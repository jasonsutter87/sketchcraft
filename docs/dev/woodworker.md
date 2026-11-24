# Woodworker pack

The Cut list grown into a shop plan: cutting diagrams on sheets and boards, part labels, cost, then (part two) CNC files
and joinery helpers. Issue #24, milestone "Makers". Part one, the cut plan, is described first; part two is at the end.

Code: `core/src/cutplan.rs` (stock, packing, cost, diagrams, labels), `core/src/cnc.rs` (CNC files), `core/src/joinery.rs`
(dado, rabbet, tenon), commands in `core/src/commands/woodworker.rs`,
app logic `app/src/cutplan.ts`, the dialog `app/src/cutplandialog.tsx`. Tests: `core/tests/cut_plan.rs`,
`app/tests/cutplan.test.ts`, `tools/ui-tests/cut-plan.mjs`; part two: `core/tests/cnc_joinery.rs`, `app/src/joinery.ts`,
`app/src/joinerydialog.tsx`, `app/tests/joinery.test.ts`, `tools/ui-tests/cnc-joinery.mjs`.

## What it builds on

- The parts are `plugins::measure`'s: every group with nothing grouped inside it and every component, measured along its
  own faces, `[length, width, thickness]`, longest first. The Cut list plugin, `parts` and `parts_layout` use the same.
- The cut plan numbers parts in `parts_layout::sort_parts` order (longest first, then material, name, id), so a number on a
  label is the same number in the Parts view.
- The Cut list plugin is still JavaScript (`core/plugins/cutlist/main.js`) and still gives its quick "to buy, roughly"
  table. The plan is the careful version, in Rust. Its sheet-or-board rule (at most 40 mm thick and at least 100 mm wide
  is sheet goods) and its lumber names (2×4, 1×6...) are the plugin's, written again in `cutplan.rs` (`is_sheet`, `lumber`):
  keep the two in step.
- Diagrams and labels are `draft::ir` pages (paths and text) written by the existing PDF and SVG writers, so text is real
  text in the PDF and fonts are the drafting fonts.

## Stock, saved with the model

`Model::cut_stock` (`CutStock`), written only when it differs from the default (`skip_serializing_if`), so a model that
never opened Cut plan saves exactly as before. A model that has stock carries the key `cut_stock`, which older builds do
not know: they open it read-only (the format guard) rather than lose it. It is in `format::KNOWN_KEYS`, in the history
skeleton (so `cut_stock` is an undo step), and ignored by the drafting dirty check (never drawn on paper).

`kerf` (default 3 mm), `trim` (0), `sheet` (2.44 × 1.22 m), `board_lengths` (8 ft), `lengths_by_size` ({"2×4": [...]}),
`currency`, and per material `{sheet, board_lengths, grain, sheet_price, board_price, board_foot_price}`. All in meters
and money. `cut_stock` merges what it is given (a material given as `null` is removed; `reset` starts again) and checks
the numbers (`CutStock::check`: kerf 0 to 50 mm, trim 0 to 0.5 m, sizes 0.05 to 20 m, prices 0 to 10 million, at most 200
materials and 32 lengths).

## `cut_plan {ids?, kerf?, trim?}`

Read-only (not an undo step; allowed in a read-only model; in the plugin and agent allowlists). `kerf` and `trim` try
other numbers without saving. Parts are grouped, and each group packed on its own:

- **Sheet goods**, by (material, thickness): guillotine packing. Parts sorted by area, then longest side, name, id. Each is
  put in the free rectangle, over all open sheets, with the smallest short-side leftover (best short side fit), then the
  rectangle splits along the shorter leftover side (SLAS), so every cut runs edge to edge. A new sheet opens only when no
  free rectangle takes the part. The kerf is paid once per cut: parts and the sheet are each grown by one kerf, so the
  last part against an edge needs none. Trim is taken off every edge first.
- **Grain**: a material with `grain: true` never turns a part (its length runs along the sheet's length). Without grain a
  part may also lie a quarter turn (`rotated: true`). Lumber (boards) always runs lengthwise. Grain is per material, not
  per part.
- **Boards**, by (material, lumber name or `T×W mm`, thickness, width): first-fit decreasing along the length. A new board
  is the shortest length sold (`lengths_by_size[name]`, else the material's `board_lengths`, else the default) that holds
  the part with its kerf; a part longer than any board is unplaced.
- Deterministic: every sort ends in name and id, groups are visited in `BTreeMap` order. Two models with the same parts
  give the same plan whatever order they were made in (the test makes one backwards).

Reply: `parts` (numbered, with `kind`, `stock`, `grain`, `on`), `sheets` (index, material, thickness, size, `parts` with
`x, y, w, h, rotated` from the sheet's top left, `waste_pct`), `boards` (index, material, stock, length, `parts` with
`x, length`, `waste_pct`, `board_feet`), `unplaced` (with a `reason`), `summary`, `cost`, and the `stock` used.

### Cost

Sheets cost `sheet_price` each. Boards cost `board_foot_price` x board feet when that is set, otherwise `board_price` each.
Board feet are the nominal size for dimensional lumber (a 2×4 8 ft long is 5.33) and the actual size for other boards, and
count the whole board bought, not just the parts. Per material and in total, to the cent. A material with no price has
`priced: false` and costs 0 (the diagrams and the dialog say so).

## Files

- `export_cut_diagrams {path .pdf|.svg, ids?, paper?}`: a summary page (counts, kerf, per-material cost, total, anything
  unplaced), a page per sheet, then boards as many to a page as fit. SVG puts the pages one under another. `paper` is `a4` or
  `letter`; by default letter for ft and in models, A4 otherwise.
- `export_cut_labels {path .pdf, ids?, paper?}`: 63.5 x 38.1 mm labels in a grid (3 x 7 on A4), one per part: number, name,
  size in the model's units, material and stock, "Sheet 2 of 3" or "Board 1 of 2", and a grain arrow (lumber and grained
  materials) or "no grain direction".
- Agents write them only inside their workspace folder, with those extensions (`policy::WRITES`). Plugins write no files.

## CNC and joinery

### `export_cnc {path .zip|.dxf | dir, ids?, tool_diameter?, dogbones?}`

Not an undo step, allowed in a read-only model. One DXF per part (`cnc::files`), numbered in `parts_layout::sort_parts`
order, so file `03-` is part 3 of the cut plan, the diagrams and the labels (a test checks the ids line up). Names are
`NN-slug.dxf` (the number padded to at least two digits, the part's name with anything but letters and digits turned into
`-`), so the same model gives the same names and, in the zip, the same bytes. `path` is a `.zip` (or a `.dxf` when there is
exactly one part); `dir` is a folder and is the person's only: `policy::vet` refuses it from agents (a plugin may not pass
paths at all). Agents write the `.zip` only inside their workspace (`policy::WRITES`), and never replace a file they did not
make. `tool_diameter` is in meters (default 6 mm, at most 100 mm).

**The part's frame** is `plugins::measure`'s, as in `parts_layout`: X along the part's length (`u`), Y across (`w`, flipped so
the drawing reads from the side that faces up when the part lies in the lay-out), origin at the lower left corner, mm,
`$INSUNITS` 4. Every face square to the thickness axis `n` is collected:

- **OUTLINE**: the biggest loop among the faces on the two big sides (a through dado splits one big face but not the
  other). Closed `LWPOLYLINE`.
- **POCKET_&lt;depth&gt;mm**: a face between the two sides is a floor. It faces the up side (normal against `n`) so was cut from
  the top, depth from the top; the other way is `POCKET_BOTTOM_<depth>mm`. When floors exist only on the underside the part is
  turned over (x mirrored, as if flipped about its width) so they are cut from the top, one setup. Islands inside a pocket
  are not drawn.
- **INSIDE**: a hole in a big face that no floor's outline matches (area and middle to 0.01 mm) goes all the way through.
  Counterbores come out as a pocket plus a through hole.
- With `dogbones`, `reliefs_for` finds inside corners: a convex corner of a hole or pocket, a reflex corner of the outline.
  Corners sharper than about 137 degrees (so not a faceted circle), corners with an edge shorter than the tool radius, and
  two corners on an edge shorter than a tool diameter are skipped (counted in `skipped_corners`). A pocket corner on the
  part's edge is also skipped (the cutter leaves the board there). The relief is an `ARC` entity of the tool's radius centered
  on the corner, going the long way round through the material, joined to `LINE`s that stop one radius short of the corner.
  A loop with a relief is lines and arcs; every other loop is one closed polyline.

The reply lists each file with its number, id, name, size, holes, pockets and reliefs.

### `dado`, `rabbet`, `tenon` `{ids: [board to cut, board that fits], ...}`

One undo step each, refused (and nothing changed) unless the boards touch. All sizes are in meters. The shared logic is
`joinery::contact`: the second board's corners are put in the first board's frame, and the face it rests on (within 0.5 mm,
overlapping by more than that on both other axes) is found; boards that share volume are told to move apart. The cut is
`csg::boolean`; the cut board keeps its name, tag, children and place in the hierarchy (`replace`, as `csg::cut` does), but
gets a new id, which the reply gives (`cut`, `fit`). `removed_volume` is the volume taken out.

- **`dado {depth?, width?, clearance?}`**: the second board must stand on an edge or end (the touching area is no wider than its
  thickness). The groove is centered on the touching area, `width` (default the board's thickness) + `clearance` wide, as long
  as the touching area (and open past an edge it reaches), `depth` deep (default a third of the board). A groove that would
  reach the board's edge is refused: that is a rabbet.
- **`rabbet {depth?, width?, clearance?}`**: the same contact, but the step runs from the nearer edge across the face to the
  far side of the second board (or `width` from the edge), `depth` deep (default half).
- **`tenon {length?, thickness?, shoulder?, clearance?}`**: the second board must meet the first with its end. A tenon
  (`thickness` default a third of the board, `shoulder` in from each edge of its width, default a sixth) of `length` (default
  60% of the first board's depth, at most 40 mm) is added to its end with a union, and the mortise (the tenon grown by
  `clearance` all round, `length + clearance` deep) is cut in the first board. It refuses a mortise that would break out of
  the face or come through. The tenon board is longer by the tenon afterwards, and so is its entry in the cut list.

The cut list and cut plan are unchanged by a dado or a rabbet (the boards' bounding sizes), a test checks that.

The app: **Cut plan… › CNC files** (tool diameter, dog-bones, one `.zip`) and **Tools › Joinery** (a small prompt with which
board is cut and the sizes in mm, or in for an imperial model; blank sizes are left to the engine). `app/src/joinery.ts` is
the pure part.

## Known limits

- Grain is a property of the material, not of each part. A part that must run across the sheet in a grained material is not
  supported yet.
- Guillotine packing is a heuristic: it is usually a little less tight than a best-effort maxrects, in exchange for cuts a
  panel saw can really make. It does not try several orders, and does not plan the cut sequence (which cut to make first).
- Board packing wastes the offcut after each board; there is no reuse of offcuts between groups.
- Prices are one number per unit, no volume breaks, no tax, one currency sign for the model.
- Parts that are not boxes (a leg turned on a lathe, a curved rail) are measured by their bounding box, like the Cut list.
- CNC: arcs and circles are not recognised: a drilled hole is a faceted polygon in the model and stays one in the DXF. Pocket
  islands are not drawn. The relief is a circle of the tool's radius centered on the corner (it overcuts one radius into the
  material), not the narrower dog-bone some CAM programs make. The up side of a part is the side opposite its biggest face's
  normal, which for a part standing on end is arbitrary; only pockets decide the turn-over.
- Joinery: a dado or rabbet needs the second board to stand on an edge or end of the first; a board lying flat is refused.
  Tenons are single and rectangular, centered across the thickness, with square shoulders on the width only; no haunches,
  no mitres, no through tenons.
