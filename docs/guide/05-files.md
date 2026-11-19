# 5. Files

## Your models

**File › Save** writes a `.sketchcraft` file. SketchCraft also keeps a live
copy as you work, so a crash doesn't lose your model.

## Files from a newer SketchCraft

Every `.sketchcraft` file says which version of the file format it uses. If you open a
file saved by a **newer** SketchCraft (for example on a computer that updated before
this one), this version can't know everything in it, and saving would quietly throw
away what it doesn't know. So the file opens **read-only**, and a bar across the top
says: "Saved by a newer SketchCraft — update to edit. You can look, but changes won't
be saved here." Files from a newer SketchCraft open read-only, so an older copy can
never damage them.

While it is read-only:

- You can orbit, measure, render and export as usual.
- Drawing, editing, Undo, **Save** and **Save As** are refused, the file on disk is
  never written to, and the live copy and the timed backups leave it alone.
- AI agents and plugins get an error that says why, instead of a silent no.
- **Restore from backup…** won't put anything over it, and a backup that is itself from
  a newer SketchCraft can't be restored by this version. **Save a copy** in that list
  still works: it copies the file byte for byte, so nothing is lost.
- A part from a newer SketchCraft can't be placed in a model from the library.
- **File › New** or **File › Open…** of another model ends it. To edit the file, update
  SketchCraft. There is no "Save a copy" of the open model itself: the copy would have
  to be made from what this version understood, which is the thing that loses data. Copy
  the file in Finder if you want a spare.

Files from before this check (with no version in them) open and save as always.

## Restoring an earlier version

SketchCraft keeps earlier copies of your model in a `Backups` folder beside it:
one every few minutes while you work (older ones are thinned to about one an hour,
then one a day), and an extra one just before anything that removes a lot: New,
Open, a big delete, or a plugin run that wiped much of the model.

**File › Restore from backup…** (or **Shift+S**, type "restore") lists them, newest
first, with the time, the size and what the copy was taken before ("before New",
"before a big shrink", "before a plugin run"). For each one:

- **Restore** puts that version in place as one step. **Undo (⌘Z) takes the restore
  back**, and the model it replaced is kept as a backup first, so you can't lose
  anything by trying.
- **Save a copy** leaves your model alone and puts that version in a `Restored`
  folder beside it, so you can open it with **File › Open…** and compare.

The same list shows work SketchCraft **set aside** for you: a `.unsaved-` file
(your changes, kept when another program saved over the model while a plugin was
running or your own save was failing) and a `.damaged-` file (a copy of a model
file that couldn't be read). Restoring from those works the same way. SketchCraft
never deletes these without telling you.

Backups made because an AI agent or a plugin did something big are kept in their
own small share (at most ten each, for a week), so they can't push out your own. Your
own keep one copy per hour for a day and one per day for a month, however many deletes
you make. A restore reads files up to 768 MB, and it works on a model over the size limits too (it opens for cutting down, see below). If the model file can't be read when SketchCraft starts, nothing is saved over it until you choose: File › Restore from backup… lists the copy that was set aside, and File › New or File › Open starts over. A set-aside file whose note was lost is still listed, marked "not verified".

If two programs edit the same model (for example Claude over MCP), **Undo** takes
back only *your* last step, never the other program's work. If the other program has
since changed or built on the same objects (or something the step would remove, such
as a colour or group, is now used by the other program), Undo says so and keeps that
step: try again after the other program takes its change back, or press Undo again to
skip the step and go one further back. Nothing is guessed.

## Models that are too big

A model can hold about two million points, edges and faces, and its file may be
about 512 MB in all (everything counts: images, photos, scans, names, notes and
component attributes; a scan is roughly 16 bytes a point, so about 30 million). An AI
agent or a plugin can take a model up to 160 MB and add 32 MB in one command; only you
can go past that. A bigger model (an enormous import, or one made before these limits)
still opens, for looking at and for cutting down; it can't be saved over your file
until it is under the limits, and the file on disk isn't touched meanwhile. To cut it
down, delete geometry or placements, and in the **Materials** panel (Textures) delete or
replace the pictures that take the most room (each shows its size) or choose **Remove
unused materials**. Names and notes may add up to 64 MB (an AI agent or a plugin may
take that no further than 2 MB, so your own naming always has room). A model that
SketchCraft saved always opens again.
Over about a million points, edges and faces everything still works, but each change
takes a second or more to save and the view takes a few seconds to redraw.

Pictures (textures, photos on a view) go up to 16,384 pixels on a side and 100 million
pixels (a 48 megapixel phone photo and an 8K texture are fine). A bigger picture is not
taken, and when one is left out of a render, an import or a file you open, the message
says which one and why.

## Opening other formats

**File › Open…** or **Import…** (⌘I):

| Format | From | Notes |
| --- | --- | --- |
| `.skp` | SketchUp 6 to 8 and 2013 onward | geometry, groups, components, colors and textures |
| `.dwg`, `.dxf` | AutoCAD and most CAD apps | DXF works on its own. DWG needs a free helper (below) |
| `.ifc` | Revit, ArchiCAD and other BIM apps | needs a free helper (below). Each element becomes a group, tagged by storey |
| `.fbx`, `.obj` | Blender, game and 3D apps | |
| `.ply`, `.las`, `.xyz`, `.pts` | 3D scans | point clouds in color; drawing tools snap to scan points |

### The DWG and IFC helpers

- **DWG:** install [LibreDWG](https://www.gnu.org/software/libredwg/) (on a
  Mac: `brew install libredwg`) or the free ODA File Converter. SketchCraft
  finds either one.
- **IFC:** SketchCraft uses [IfcOpenShell](https://ifcopenshell.org). Open an
  IFC file without it and SketchCraft shows the exact commands to set it up.

## From a floor plan to walls

Import a 2D floor plan (DWG or DXF), right-click it and choose **Raise walls**:
the walls go up, with openings where the windows are.

## Exporting

**File › Export ▸**: glTF (`.glb`), FBX (`.fbx`), OBJ, STL (for 3D printing), Collada
(`.dae`), 3D DXF, DWG, IFC, a PDF or PNG of the view, a scene tour (MP4), and
section cuts (SVG, DXF). Also on the File menu: **Make blueprint (PDF)**,
**Drawing sheets (PDF)** at true scale, and **Generate report (CSV)**.

**FBX** is for game engines and 3D tools (Unity, Unreal, Blender, Maya). Every group and
component becomes its own object, nested the way it is nested in your model and
keeping its name. Faces come out as triangles with normals, texture coordinates and
materials (colour, finish). Pictures used as textures are saved beside the file in a
`<name>_textures` folder and linked by a relative path, so keep the two together.
Units are metres and Y is up. It is binary FBX (7.4); agents can ask for the text
version with `binary: false` (Blender doesn't open text FBX).

Next: [Presenting](06-presenting.md)
