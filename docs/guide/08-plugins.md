# 8. Plugins

![The Shelves and Cut list plugins](../demos/shelves-cut-list.gif)

Plugins add commands to SketchCraft. Find them in **File › Plugins ▸** or by
name in **Shift+S**.

## Built in

- **Cut list:** select the boards (each its own group or component), or select
  nothing for the whole model. It lists every part to cut, grouped by size and
  material, and roughly how many 4×8 sheets and 8-foot boards to buy. Boards
  that are real dimensional lumber show by name (2×4, 1×6, 4×4…), and every
  board line has its board feet, with a total. Copy the table or save it as CSV.
  The **Cut plan…** button on its result (also **Tools › Cut plan…**) goes on from
  there: see [Cut plan](#cut-plan).
- **Shelf unit:** a bookcase from a few sizes, every board its own group, ready
  for the cut list.

## Cut plan

**Tools › Cut plan…** packs the cut list's parts onto real sheets and boards, for
what you have picked or the whole model. It has four views:

- **Cutting diagram:** each sheet and board drawn to scale with every part
  numbered and sized. Parts are numbered longest first, the same order as the
  Parts view. Parts that fit nowhere (bigger than a sheet, or longer than any
  board) are listed in red, never dropped quietly.
- **Stock & prices:** the saw **kerf**, the **trim** cut away around a sheet's
  edge, the sheet size (default 4×8 ft, 2440 × 1220 mm), the board lengths you
  can buy, and per material a price per sheet, per board or per board foot, and
  whether it has a **grain**. A material with grain keeps every part's length
  along the sheet's length; without it parts may turn a quarter turn to fit
  better. A price per board foot wins over a price per board. All of it is saved
  with the model, and **Update plan** is one Undo step.
- **What to buy:** sheets, boards, board feet and the cost per material, and the
  total. A material without a price is marked, and counts as zero.
- **CNC files:** one DXF per part for a router (see [CNC files](#cnc-files)).

**Export diagrams (PDF)** writes a summary page, a page for each sheet and pages of
boards; **Export diagrams (SVG)** writes the same pages one under another.
**Print labels (PDF)** writes sheets of 63.5 × 38.1 mm labels (21 to an A4 page),
one for each part: its number, name, size, material, which sheet or board it is
cut from, and a grain arrow. Sheets are packed so every cut runs from edge to
edge, which a panel saw can do.

### CNC files

**Export CNC files…** (the fourth view of the Cut plan) saves a `.zip` with **one
DXF drawing per part**, in millimeters, named by the part's number and name
(`01-Side-left.dxf`; the number is the one on the cutting diagram and the
labels). Each part is drawn flat, seen from the side that faces up when it lies
on the table, with its lower left corner at the origin:

- layer **OUTLINE**: the part's edge;
- layer **INSIDE**: holes that go all the way through;
- layers **POCKET_6.0mm** and so on: floors cut from the top, named by their
  depth, such as a dado or a rabbet (**POCKET_BOTTOM_…** when cut from the
  underside; if every pocket is on the underside the part is turned over so they
  are cut from the top).

Type the **tool diameter**. With **Dog-bone corners** ticked, every inside
corner gets a relief arc of the tool's radius centered on the corner, so a
round bit can reach it: the corners of pockets and holes, and notches in the
outline. A pocket corner at the edge of the part (a dado that runs off the
board) is left alone, because the bit can leave there.

### Joinery

**Tools › Joinery** cuts joints between **two boards that touch** (pick both,
then choose; you say which one is cut and the other one fits into it). Each is
one Undo step, leaves a solid, and keeps the cut list right: a dado or a rabbet
does not change a board's size, and a tenon makes its board longer by the tenon,
which is the length to cut.

- **Dado:** a groove in the face of one board, as wide as the other is thick,
  where it stands on it. Depth defaults to a third of the board. A groove
  that would run into the board's edge is refused (use a rabbet).
- **Rabbet:** a step along the nearest edge of one board, out to the far side of
  the other board, where it stands near that edge. Depth defaults to half.
- **Tenon and mortise:** a tongue on the end of one board (length, thickness
  and **shoulder**, how far in from each edge of its width) and the slot for it
  in the board it meets; **clearance** gives the slot a little room all round.

Boards that don't touch, that overlap, or where a board lies flat instead of
standing on an edge or an end, are refused with a message, and nothing changes.

## Adding and managing plugins

**File › Plugins ▸ Manage plugins…** lists them all. Turn each on or off,
remove ones you added (they're moved aside, not deleted), and **Install from
file…** to add a plugin someone sent you as a `.zip`.

## SketchUp Ruby scripts

Many SketchUp extensions that **build** things (stairs, framing, fences,
shelves, houses) run in SketchCraft. Install the `.rbz` (or `.rb`) with
**Install from file…**; its menu items appear in **File › Plugins**. Input boxes
become SketchCraft forms, and what the script builds can be undone.

Extensions that need SketchUp's own windows, mouse tools or files won't run
yet; they stop with a message saying what's missing.
[What works](../dev/sketchup-ruby.md).

## Are plugins safe?

They run in a sandbox: no internet, no access to your files, no way to run other programs, and a minute at
most. SketchUp Ruby scripts run in the same sandbox. They can change your model, and **⌘Z** undoes that like anything else.

## Write your own

A plugin is a little JavaScript. See [Writing a plugin](../PLUGINS.md).

Next: [Keyboard shortcuts](09-shortcuts.md)
