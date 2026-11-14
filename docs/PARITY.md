# SketchUp parity

Goal: everything SketchUp Pro / Studio users pay for, rebuilt clean-room.
Checked items are in `main`.

## Modeling

- [x] Line, arc, rectangle, circle, polygon, push/pull, offset, follow me
- [x] Move / rotate / scale, copies and arrays (`x5`), Flip Along, typed exact values in any units
- [x] Inference: endpoints, midpoints, on edge/face, axes, From Point; arrow-key and Shift locks
- [x] Lines closing a loop on a face cut a new face (windows on walls)
- [x] Groups, nested components, tags, hide/unhide, soften, outliner, entity info
- [x] **Solid Tools**: outer shell, union, intersect, subtract, trim, split; solid check and volume
- [x] **Intersect Faces** (with model / with selection)
- [x] **Sandbox**: from contours, from scratch, smoove, stamp, drape, flip edge
- [x] Tape measure and **Protractor** guides, dimensions, text, **3D Text** (any system font, extruded or flat), units (m/cm/mm/ft-in/in)
- [x] Curves: arcs and circles pick as one; **Weld** and **Explode Curve**; **Make Unique**; **Orient Faces**
- [x] Camera tools: **Position Camera**, **Look Around**, **Walk** (walls stop you, stairs climb), **Zoom Window**, previous / next view, typed eye height and field of view (`35` or `50mm`)
- [x] **Dynamic Components**: attributes and formulas (lengths, copies, material, hidden, `parent!`)
- [x] Keyboard shortcuts 1:1 with SketchUp's defaults (press `?`)

## Looks and presentation

- [x] Materials and image textures (tile size, projected)
- [x] **Styles**: shaded / textured / monochrome / hidden line / wireframe, profiles,
      sketchy edges, extensions, ambient occlusion, paper / blueprint / night; presets
- [x] Scenes (saved views), section cuts with **section fills**, X-ray, back edges, shadows, parallel projection
- [x] **Match Photo**: camera from a photo's vanishing lines; the scene keeps the photo
- [x] **Scene animation** export (MP4, or a turntable)
- [x] **LayOut-lite drawing sheets**: viewports at true scale, scale bars, title blocks, multi-page PDF

## Files

- [x] Import: SketchUp `.skp` (6–8 and 2013+), OBJ (+ .mtl), binary FBX
- [x] Export: glTF, OBJ, STL, Collada (.dae), 3D DXF, PDF, section cuts (SVG / DXF), IFC4 (BIM)
- [x] **Classifier** (IFC types) and **Generate Report** (CSV)
- [x] **Add Location**: real terrain and OpenStreetMap imagery around a lat / lon

## Also

- [x] Headless + MCP: every action is a command Claude can run
- [x] **SubD modeling**: edit a subdivision cage (points / edges / faces, symmetry, soft select, loops, extrude, crease) over see-through reference images

## Known gaps (next)

- DWG (binary AutoCAD) import/export: DXF covers interchange today
- Textures in exports and headless renders (those use each material's color)
- Photo-textures projected from a matched photo onto faces
- Dynamic Component position / rotation attributes (x, y, z, rotz) and option dialogs
- IFC checked structurally (tests), not yet against IfcOpenShell / Revit
- Point clouds (Scan Essentials), V-Ray-class rendering, Revit import (Studio)
