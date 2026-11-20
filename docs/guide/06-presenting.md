# 6. Presenting

## Styles

**Style** in the right panel: shaded, shaded with textures, physically based,
monochrome, hidden line or wireframe, on a sky, paper, blueprint or night
background. Add profiles, sketchy edges, extensions or ambient occlusion. The
**Preset** list has ready-made looks.

## Painting (B)

Pick a color or a texture on the right, then click a face or a group with the
**Paint Bucket**. Modifier keys reach further, as in SketchUp:

| Hold | What gets painted |
| --- | --- |
| nothing | the face (or group) you click |
| **Ctrl** (Option on a Mac) | every face touching it that has the same material. The spread stops at a face with any other material and crosses hard and soft edges alike |
| **Shift** | every face in the same group with the same material (a replace) |
| **Ctrl + Shift** | every face with the same material on the whole connected object, even past faces of other materials |
| **Alt** (Windows, Linux) or **Cmd** (Mac) | pick up the color or texture under the pointer |

**Paint-through.** When a texture is painted across touching faces, it runs on
as one sheet: around a corner or along a folded strip, each face's picture is
unrolled from its neighbour's across the shared edge, with the same tile size,
lined up on the edge, so bricks and wood grain keep going instead of restarting
on every face. The clicked face keeps the texture's usual placement and the
rest continue from it. A closed shape such as a box can't unroll flat, so the
seam where the sheet meets itself again can still step. Plain colors just fill.
Each click is one undo step. Agents reach the same thing with
`paint {ids, material, scope: "connected" | "all" | "object", through}`.

## Vertex painting

**Tools > Vertex Paint** (or press `Shift+S` and search for it) paints colour onto the *corners* of faces, the way a game
artist tints a mesh. Pick a colour with the paint colour chip, then drag over the model: a circle on the surface shows
the brush, and every corner inside it moves toward the colour, most at the middle and less toward the rim. The faces
blend the corner colours smoothly across themselves, and the colour multiplies with the face's material (a plain white
face shows it as painted; a picture shows through it).

| Do | What happens |
| --- | --- |
| drag | paint a stroke; letting go finishes it, and one Undo takes the whole stroke back |
| **[** / **]** | a smaller / bigger brush (or use the panel's Size) |
| **Alt** + drag | brush the colour away |
| the panel's **Strength** and **Edge** | how hard the brush pushes, and whether it fades (soft, linear) or stops dead (hard) |
| **Fill selection** / **Clear selection** | colour every corner of the selected faces, groups or components, or take the colours off |
| **View > Vertex colours** | show or hide them (on by default) |

Colour lives on corners, so a big flat face with only four corners can only be tinted from corner to corner. Cut it up
(or use a finer model such as the globe) to get a soft, local brush. The colours are saved with the model, appear in
renders, and go into glTF / GLB (as vertex colours) and FBX (as a colour layer). Agents reach the same thing with
`vertex_paint`, `vertex_paint_clear` and `vertex_colors`.

## Scenes

In the **Views** tab, name the view and click **Save**: it remembers the camera,
and the style, hidden things, tags and section cut that go with it (in the
Previz strip, **+ Scene** does the same). Click a saved view to glide back.

## Sections and plans

**X** (Section cut) and click a face to slice the model open. **Plan** in the
view bar cuts a floor plan; in the **Arch** workspace, the floor buttons cut
each floor.

## Settings around the model

![Environments](../demos/environments.gif)

**Library › Environments** (or Setting in the Arch and Previz strips) puts the
model on a suburban lot, a city block, a park, a desert or a waterfront.

## Photoreal renders

![Studio renders](../demos/studio-render.gif)

**Render** (top right) makes a photoreal picture that sharpens while you
watch. Pick a quality, place the sun (for this view, by hand, or the real sun at
a date and time), and **More settings** has size (HD, 4K, square, portrait,
custom), exposure, sunlight, sky light, sun color, light bounces, a plain
background color and the ground. **Finish** in the right panel sets each
material: matte, satin, gloss, metal, glass, glow.

## Video and sun studies

The **Timeline** (Previz workspace) makes turntables, walkthroughs and sun
studies, exported as MP4.

## Drawings

**File › Make blueprint (PDF)** and **Drawing sheets (PDF)** lay out views at
true scale with a title block. Plans, elevations, sections and parallel 3D views are
drawn as crisp vector lines with the hidden lines removed (add `"beyond": true` to a
viewport in `sheets`/`export_sheets` to show them dashed), so they stay sharp at any size
and print or open in a CAD program. Sheets come in A4 to A0, Letter, Tabloid, ARCH A to E
and ANSI C to E; a scale is `1:100` or imperial like `1/4"=1'-0"`. Line weights are pens
in paper millimeters (0.13 to 0.5), so they don't change with the scale. The `export_sheets`
command writes a `.pdf` (every page), or one page as `.svg` or `.dxf` (`"page": 2`).

Next: [Themes and workspaces](07-themes.md)
