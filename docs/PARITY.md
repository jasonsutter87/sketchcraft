# SketchUp parity

Goal: everything SketchUp Pro / Studio users pay for, rebuilt clean-room.
Checked items are in `main`.

## Modeling

- [x] Line, **Freehand**, arcs (**Arc** from its center, **2-Point**, **3-Point**, **Pie**), rectangle, **Rotated Rectangle**, circle, polygon, push/pull, offset, follow me
- [x] Select like SketchUp: double-click a face for it and its edges, triple-click for everything connected; select all with the same paint or tag
- [x] **Axes tool** (turned drawing axes for buildings set at an angle); Tape Measure **rescales the model**
- [x] Doors and windows **cut openings** (through thick walls, with reveals); any component can be set to
- [x] **Hide Rest of Model** while editing; **Paste in Place**; **command search** (Shift+S) and outliner search
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
- [x] **Dynamic Components**: size, copies, material, hidden, position (x, y, z) and rotation (rotx, roty, rotz), formulas (`parent!`), and Component Options (text boxes, lists, read-outs)
- [x] Keyboard shortcuts 1:1 with SketchUp's defaults (press `?`)

## Looks and presentation

- [x] **Paint Bucket modifiers** (Ctrl/Option connected, Shift replace all, Ctrl+Shift whole object, sample) and **paint-through**: a texture runs on across connected faces
- [x] **Vertex painting**: a brush that tints the corners of faces (size, strength, soft or hard edge, Alt erases), shown in the viewport and renders and exported as glTF `COLOR_0` and FBX colours
- [x] Materials and image textures (tile size, projected; **projection mapping**: planar, box, cylinder and sphere onto a selection, kept as the faces' own texture coordinates; **Position Texture**: pins to move, scale, turn and skew the picture on a face, with flips and 90-degree turns; **UV editor**: seams, LSCM unwrap into packed islands, a 2D view to select, move, turn, scale, pin and repack them, and a second **lightmap UV set** exported to glTF and FBX); **physically based finishes** (Matte, Satin, Gloss, Metal, Brushed metal, Glass, Glow) in renders, glTF and a physically based view style
- [x] **Styles**: shaded / textured / monochrome / hidden line / wireframe, profiles,
      sketchy edges, extensions, ambient occlusion, paper / blueprint / night; presets
- [x] Scenes (saved views) that remember style, tags, hidden things and the section cut; **named section planes** with **section fills**, X-ray, back edges, shadows, parallel projection
- [x] **Match Photo**: camera from a photo's vanishing lines; the scene keeps the photo; **Project Photo** onto faces as straightened textures
- [x] **Timeline** for video: turntables, recorded walkthroughs, glides through scenes, and sun studies (built-in solar position, clock on the frame); previewed live, exported as MP4 at 720p / 1080p / square / vertical with viewport shadows
- [x] **LayOut-lite drawing sheets**: viewports at true scale, scale bars, title blocks, multi-page PDF

## Files

- [x] Import: SketchUp `.skp` (6–8 and 2013+), **DXF** (layers as tags, blocks, units, text as labels, dimensions, splines, ellipses, hatches), **DWG** (via LibreDWG / ODA converter; tested on AutoCAD 2004 and 2018 files), OBJ (+ .mtl), binary FBX, **IFC** (Revit / ArchiCAD exports; through IfcOpenShell: elements typed for the Classifier, grouped and tagged by storey, openings cut)
- [x] Export: glTF, **FBX** (binary or ASCII; UVs, materials, textures, nested groups), OBJ, Collada (all with textures), STL, 3D DXF, **DWG** (via converter), PDF, section cuts (SVG / DXF), IFC4 (BIM)
- [x] **Raise walls** from an imported floor plan (wall bands found across T-junctions, window lines and drafting slips; window openings with sill and head)
- [x] **Point clouds** (Scan Essentials core): PLY, LAS, XYZ / PTS / TXT; colored, thinned to a budget, survey coordinates moved home; drawing tools snap to scan points
- [x] **Classifier** (IFC types) and **Generate Report** (CSV); IFC export validated with IfcOpenShell (`tools/ifc/check_ifc.py`)
- [x] **Add Location**: real terrain and OpenStreetMap imagery around a lat / lon

## Also

- [x] Headless + MCP: every action is a command Claude can run
- [x] **SubD modeling**: edit a subdivision cage (points / edges / faces, symmetry, soft select, loops, extrude, crease) over see-through reference images

## Audit, 2026-11-14

SketchUp's menus and toolbars (free, Pro, Studio) checked against the code,
not against this list. Ranked by how often a SketchUp user would hit it.

**P1: everyday modeling**: all done (2026-11-14), now in the lists above.

**P2: looks and files**

| Gap | Today |
| --- | --- |
| Roughness / normal **maps** (images), and HDRI lighting | per-material roughness, metalness, glass, glow (done) |
| **Fog**, **watermarks** | missing |
| **Tag folders**, dashed tag lines, Tag tool | flat tags |
| Components that **face the camera** (2D people and trees) | missing |
| Import images (as an image, a texture or a photo to match); 3DS, KMZ | Match Photo only |
| Export FBX, KMZ, USDZ | missing |

**P3: Pro / Studio depth**

| Gap | Today |
| --- | --- |
| A learned denoiser (Intel OIDN) and GPU tracing (V-Ray-class) | CPU path tracer with an edge-aware (albedo / normal guided) denoiser |
| LayOut depth: dimensions and text on sheets, scrapbooks, pattern fills | layout-lite PDF |
| An extension API | MCP commands only |

## Known gaps (next)

- DWG needs a converter installed (LibreDWG or ODA); a native DWG reader is out of scope
- Revit (.rvt) is a closed format with no public reader: open Revit's IFC export instead. IFC import needs IfcOpenShell installed (one-time steps in the app's message)
- Rendering is a CPU path tracer with a built-in edge-aware denoiser; no learned denoiser (OIDN) or GPU tracing yet, so not V-Ray-class
- Point clouds: LAZ (compressed) needs converting to LAS first; scans aren't drawn in headless renders
- Raise walls works from wall layers (or the whole drawing); walls drawn only as single lines aren't thickened
