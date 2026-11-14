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
- [x] **Match Photo**: camera from a photo's vanishing lines; the scene keeps the photo; **Project Photo** onto faces as straightened textures
- [x] **Timeline** for video: turntables, recorded walkthroughs, glides through scenes, and sun studies (built-in solar position, clock on the frame); previewed live, exported as MP4 at 720p / 1080p / square / vertical with viewport shadows
- [x] **LayOut-lite drawing sheets**: viewports at true scale, scale bars, title blocks, multi-page PDF

## Files

- [x] Import: SketchUp `.skp` (6–8 and 2013+), **DXF** (layers as tags, blocks, units, text as labels, dimensions, splines, ellipses, hatches), **DWG** (via LibreDWG / ODA converter; tested on AutoCAD 2004 and 2018 files), OBJ (+ .mtl), binary FBX
- [x] Export: glTF, OBJ, Collada (all with textures), STL, 3D DXF, **DWG** (via converter), PDF, section cuts (SVG / DXF), IFC4 (BIM)
- [x] **Raise walls** from an imported floor plan (wall bands found across T-junctions, window lines and drafting slips; window openings with sill and head)
- [x] **Point clouds** (Scan Essentials core): PLY, LAS, XYZ / PTS / TXT; colored, thinned to a budget, survey coordinates moved home; drawing tools snap to scan points
- [x] **Dynamic Components**: size, copies, material, hidden, position (x, y, z) and rotation (rotx, roty, rotz), formulas, and Component Options (text boxes, lists, read-outs)
- [x] **Classifier** (IFC types) and **Generate Report** (CSV)
- [x] **Add Location**: real terrain and OpenStreetMap imagery around a lat / lon

## Also

- [x] Headless + MCP: every action is a command Claude can run
- [x] **SubD modeling**: edit a subdivision cage (points / edges / faces, symmetry, soft select, loops, extrude, crease) over see-through reference images

## Known gaps (next)

- DWG needs a converter installed (LibreDWG or ODA); a native DWG reader is out of scope
- Revit (.rvt) is a closed format with no public reader; the practical route is IFC import (Revit exports IFC), not built yet
- Rendering is a CPU path tracer: no denoiser or GPU tracing yet, so not V-Ray-class
- Point clouds: LAZ (compressed) needs converting to LAS first; scans aren't drawn in headless renders
- Raise walls works from wall layers (or the whole drawing); walls drawn only as single lines aren't thickened
