# Roadmap

Where SketchCraft is going next, and why. Each release has a theme, built from
what people who use SketchUp every day say is missing: in forums, reviews, on
Reddit and Discord, and in the issues here.

Every item is a GitHub issue in its release's
[milestone](https://github.com/jasonsutter87/sketchcraft/milestones). 👍 the ones
you want, and open a [feature request](https://github.com/jasonsutter87/sketchcraft/issues/new/choose)
for anything missing. Plans change as we learn; the order below is our best
guess today.

| Release | Theme | In a sentence |
| --- | --- | --- |
| **0.3.6** | Opens your files | SketchUp files from any version (6 to 27), a security review, faster and steadier everywhere |
| **0.3.7** | Safety & polish | Older versions can't damage newer files, a Subdivide button, a contributor's Trim fix and Vertical layout, and the full test suite in CI |
| **0.3.8** | Drafting | LayOut-class drawings: line weights, hatches and cut fills per tag, dimensions that stay attached, big documents that stay fast |
| **0.3.9** | Texturing | A blocky beaver to paint, then put textures exactly where you want them, unwrap UVs, paint vertices, export to game engines |
| **0.3.10** | Makers | Fix a model in one click, check it for 3D printing, cut lists with diagrams and CNC files |
| **0.4.0** | No extension tax | The add-ons people pay for most, built in: round corners, loft, thicken, bend, parametric railings and stairs |
| **0.4.1** | Big models | Every CPU core, lighter imports, smooth orbiting on huge sites |
| **0.4.2** | Switchers & your own AI | Coming from SketchUp made easy, your old extensions, and AI without credits |
| **0.4.3** | Rendering | Clean, fast renders on the GPU with HDRI lighting, no paid plugin |

## 0.3.7: Safety & polish

A small, solid release before drafting:

1. **Format guard**: an older SketchCraft opens a newer file read-only instead of silently dropping what it doesn't understand
2. **Subdivide button** and a SubD box starter
3. **Trim keeps nested hardware** and the **Vertical tools layout** (thanks, Kad26)
4. **The test suite in CI**: UI tests on every PR, a test for every tool, unit tests, screenshot comparisons, SketchUp's own test files

## 0.3.8: Drafting

The biggest reason professionals stay on SketchUp is LayOut, and the biggest
complaint about LayOut is that it's slow and loses your dimensions. So:

1. **Big documents stay fast**: A1 size, 50+ pages
2. **Dimensions and viewports stay attached** when the model changes
3. **Line weight and line style per tag and per edge**
4. **.pat hatches per tag**, with colour, thickness, scale and angle
5. **Cut fill per tag** in sections
6. **Opacity per viewport**, for hybrid drawings without fog
7. **Trim, extend and continuous dimensions** on sheets
8. **An interactive sheet editor**
9. **AI drafting**: "dimension this plan", "lay out a permit set"
10. **Tables and text blocks**

## 0.3.9: Texturing

1. **Blocky Beaver** and a **texturing test set** (dice, can, globe, crate, soccer ball): every feature below is built and tested on them
2. **glTF / GLB export** with UVs and vertex colours, plus **FBX** and a **lightmap UV channel**
3. **UV unwrap** with a UV view
4. **Position Texture**: move, rotate, scale and skew a texture on a face
5. **Paint-through** across connected faces
6. **Projection mapping**: planar, box, cylinder, sphere
7. **Textures in SketchUp 2017–2020 imports**
8. **Vertex painting** (done: Vertex Paint tool, vertex colours in the viewport, renders, glTF / GLB and FBX)

## 0.3.10: Makers

1. **Fix model**: holes, stray edges, internal faces, reversed faces, merged coplanar faces, in one undoable step
2. **3D printing check** and **3MF export**
3. **Woodworker pack**: cutting diagrams, labels, cost, CNC parts, joinery helpers
4. **Small tools**: Bezier curves, make faces, flatten to a height

## 0.4.0: No extension tax

1. **Round corners** on any edge and corner, concave too
2. **Loft** between profiles
3. **Thicken** and **Joint Push/Pull**
4. **Bend, taper and twist**
5. **Parametric profiles, railings and stairs** that remember their settings

## 0.4.1: Big models

1. **Multithreaded** booleans, imports, scene building and exports
2. **Decimate heavy imports**
3. **Instancing and level of detail** in the viewport
4. **Incremental viewport updates**
5. **A public benchmark**, run on every release

## 0.4.2: Switchers & your own AI

1. **A SketchUp import report**: what came in and what didn't
2. **Run the top free SketchUp extensions**, with a compatibility table
3. **Save as older SketchUp versions** (researching)
4. **Bring your own AI**: connect Claude, Cursor or a local model, no credits, with ready-made recipes
5. **Coming from SketchUp Make 2017** guide
6. **An open CC0 asset library**

## 0.4.3: Rendering

1. **AI denoiser**: clean renders in a fraction of the time
2. **GPU path tracing**
3. **HDRI lighting** and **roughness, metalness and normal maps**

## Always

- **Free and open source**, offline, on macOS, Windows and Linux. No subscription,
  no account, no credits.
- **Your files are yours**: open any SketchUp version, save in an open format.
- **Every release is attacked before it ships** (see [SECURITY.md](SECURITY.md)).
