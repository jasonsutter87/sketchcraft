# SketchUp parity

Goal: everything SketchUp Pro / Studio users pay for, rebuilt clean-room.
Checked items are in `main`; the rest is the plan, in order.

## Done

- [x] Drawing: line, arc, rectangle, circle, polygon, push/pull, offset, follow me
- [x] Move / rotate / scale, copies and arrays (`x5`), typed exact values in any units
- [x] Inference: endpoints, midpoints, on edge/face, axes, From Point, arrow-key and Shift locks
- [x] Groups, components (nested), tags, hide/unhide, soften, outliner, entity info
- [x] Tape measure guides, dimensions, text, units (m/cm/mm/ft-in/in)
- [x] Materials and image textures (tile size, projected), paint sampling
- [x] Scenes (saved views), section cut, X-ray, back edges, shadows, parallel projection
- [x] **Solid tools** (Pro): outer shell, union, intersect, subtract, trim, split; solid check and volume
- [x] **Keyboard shortcuts 1:1 with SketchUp's defaults** (press `?` in the app)
- [x] Import: SketchUp `.skp` (6–8 and 2013+), OBJ (+ .mtl); export: glTF, OBJ, STL
- [x] Headless + MCP: every action is a command Claude can run

## Next phases

1. **Intersect Faces** (with model / with selection) and **Flip Along**
2. **Section fills** and section-cut export (SVG / DXF)
3. **Sandbox**: terrain from contours, from scratch, smoove, stamp, drape, flip edge
4. **Exports**: DAE (Collada), DXF/DWG 2D+3D, high-res PNG, PDF; FBX import
5. **Scene animation** export to video, with transitions
6. **Match Photo**: line a photo up with the model's axes
7. **Styles**: sketchy edges, profiles, ambient occlusion, watermarks
8. **LayOut-lite**: sheets with live model viewports, scaled drawings, title blocks, PDF
9. **Dynamic components** (attributes, formulas) and **Generate Report** (quantities)
10. **Classifier / IFC** export; **geo-location** with terrain
