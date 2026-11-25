# 11. Coming from SketchUp

SketchCraft is an independent program that works the way SketchUp does and reads its files. This page is for someone
who already knows SketchUp, including the free SketchUp Make 2017: what carries over, what to do first, and what is
different. It is not affiliated with Trimble or SketchUp.

## Your first ten minutes

1. **Open a model.** **File › Open…** (⌘O) and pick your `.skp`. See [what opens](#importing-your-old-models) below.
2. **Bring your shortcuts.** Press **?**, then **Customize…**, and choose the **SketchUp** preset
   ([Keyboard shortcuts](09-shortcuts.md)).
3. **Draw and type sizes as you always did.** Type the size while a tool is running and press Enter (`5,4`, `8'6"`,
   `250cm`): see [Drawing](02-drawing.md).
4. **Find anything by name** with **Shift+S** if you can't find a command.

## Tools side by side

| SketchUp | SketchCraft | Key | Notes |
| --- | --- | --- | --- |
| Select | Select | Space | hold Space to orbit |
| Line | Line | L | closing a loop makes a face |
| Rectangle | Rectangle | R | type `width,depth` |
| Rotated Rectangle | Rotated Rectangle | | |
| Circle | Circle | C | type `24s` for 24 sides |
| Polygon | Polygon | N | the SketchUp preset leaves this key off, as SketchUp does |
| Arc, 2 Point Arc, Pie | 2-Point Arc, Arc (center), 3-Point Arc, Pie | A (2-Point) | the others are on the tool bar or Shift+S |
| Freehand | Freehand | | |
| Push/Pull | Push/Pull | P | |
| Move | Move | M | Option/Ctrl copies; `x5` makes five copies |
| Rotate | Rotate | Q | |
| Scale | Scale | S | |
| Follow Me | Follow Me | J | the SketchUp preset leaves this key off |
| Offset | Offset | F | |
| Tape Measure | Tape Measure | T | a click on an edge, then pull out for a guide line |
| Protractor | Protractor | | an angled guide |
| Dimensions | Dimension | D | |
| Text | Text | W | |
| Paint Bucket | Paint | B | Ctrl/Option, Shift and Ctrl+Shift work as in SketchUp ([Presenting](06-presenting.md)) |
| Eraser | Erase | E | Shift hides an edge instead of deleting it |
| Section Plane | Section cut | X | |
| Orbit, Pan, Zoom | Orbit, Pan, Zoom | O, H, Z | |
| Zoom Extents | Zoom to fit | Shift+Z | |

Sources: the tool tables in [Drawing](02-drawing.md), [Shaping](03-shaping.md) and [Keyboard shortcuts](09-shortcuts.md).

## Shortcuts

SketchCraft's default keys are SketchUp's. The **SketchUp** preset is SketchUp's own set: the same as SketchCraft's,
except Polygon, Follow Me, Dimension, Text and Section Plane, which SketchUp leaves without a key. You can also read a
shortcuts file exported from SketchUp (Preferences › Shortcuts › Export) with **Import / export**; SketchCraft lists
the commands it could not match. Details are in [Keyboard shortcuts](09-shortcuts.md#make-them-yours).

## Importing your old models

**File › Open…** and **File › Import…** (⌘I) read `.skp` files from SketchUp 6 to 8 and from 2013 onward, which covers
files saved by SketchUp Make 2017. You get the geometry, groups, components, colors and textures (the pictures, at the size SketchUp gives them)
([Files](05-files.md#opening-other-formats)). Also:

- **Dimensions** (linear ones) come in as dimensions attached to what they measure. Radial dimensions, free text labels
  with leaders and annotations in 2021 and later files are left out; the import result says how many.
- **Guide lines** and **section planes** are read so the file opens, but they are not placed in the model.
- Files you save are `.sketchcraft` files. SketchCraft exports glTF, FBX, OBJ, STL, 3MF, Collada, DXF, DWG, IFC, PDF,
  PNG and MP4, but does not write `.skp`, so keep your originals if you also still use SketchUp.
- Other CAD and BIM formats: DXF, DWG and IFC ([Files](05-files.md)).

If a file does not open, write it up on the [issue tracker](https://github.com/jasonsutter87/sketchcraft/issues) (there
is an import problem form); the original `.skp` is what helps most.

## Extensions that run

SketchCraft runs SketchUp Ruby scripts (`.rb`) and extensions (`.rbz`) that **build** geometry, such as generators for
stairs, framing, shelves and fences. Install one with **File › Plugins ▸ Manage plugins… › Install from file…**; its
menu items appear in **File › Plugins** and in Shift+S. They run in a sandbox with no network or file access, and ⌘Z
undoes what they build ([Plugins](08-plugins.md#sketchup-ruby-scripts)).

It is a subset of SketchUp's Ruby API. These do not run yet: mouse-driven tools (`Sketchup::Tool`), observers,
`UI::HtmlDialog` and `WebDialog`, timers, `followme`, textures, reading the existing model's geometry beyond the
selection, files, and `eval` of strings. A script that needs one stops with a message saying what is missing.
The full list is in [SketchUp Ruby scripts in SketchCraft](../dev/sketchup-ruby.md).

## What is different

- **Tags** are SketchUp's layers: select something, give it a tag in the Outliner, then hide, show, solo or color by
  tag ([Organizing](04-organizing.md#tags)).
- **The Outliner** is on the left and is where you drag things into groups.
- **The right panel** holds Style, paint and, for components with attributes, Component Options (like Dynamic Components).
- **The Classic look** is the default for new models: a SketchUp-like sky, ground and edges. Other looks are under
  Style and Themes ([Presenting](06-presenting.md#styles), [Themes](07-themes.md)).
- **Drawing sheets** (a LayOut-like PDF at true scale) are built in, but with fewer features than LayOut: the
  [parity list](../PARITY.md) says what is still missing.
- **Backups and Undo.** SketchCraft keeps earlier copies of your model; **File › Restore from backup…** brings one back
  ([Files](05-files.md#restoring-an-earlier-version)).
- **AI agents.** Every action is also a command an AI agent can run ([AI agents over MCP](../dev/mcp.md)).

For everything that SketchUp has and SketchCraft does not yet, read [SketchUp feature parity](../PARITY.md).

Next: [Troubleshooting](10-troubleshooting.md)
