# SketchUp extensions: what runs

SketchCraft runs SketchUp Ruby scripts and extensions (`.rb`, `.rbz`) in its plugin sandbox: no files, no network,
no way out ([how it works](../dev/sketchup-ruby.md)). This page says which popular free extensions run today. It is
kept honest: **"works" means the real extension, unchanged, was loaded and driven in a test**; anything else says
what was and wasn't checked.

How to read the status:

- **works**: the real extension loads and does its main job in the tests.
- **partly**: the real extension loads and some of it works; the notes say what doesn't.
- **doesn't load**: it stops before it can do anything; the notes say why.
- **calls covered**: we have not run the real extension (it is encrypted, paid, or needs a login to download), but a
  test fixture written for this project makes the same Ruby API calls it is known for, and they pass. This is
  *not* a promise that the real one runs.
- **not tested yet**: nobody has tried it.

The list below is the extensions SketchUp itself and the community point new people to first. It is an editorial
list, **not a download-count ranking** (the Extension Warehouse doesn't publish one); the first five are the ones
issue [#37](https://github.com/jasonsutter87/sketchcraft/issues/37) named.

## The table

Tested with SketchCraft 0.3.10 plus the work for #37. Versions are the ones tested.

| # | Extension | Version tested | Status | Notes |
| --- | --- | --- | --- | --- |
| 1 | **CleanUp³** (Thomas Thomassen; MIT, source on GitHub) | 3.4.6 | **doesn't load** | It needs **TT_Lib²** (a separate extension of Thomas's) first, and we could not get TT_Lib²'s source to test with. Past that, its jobs (merge coplanar faces, erase stray edges and hidden geometry, repair split edges) need to change edges of what is already in the model; SketchCraft's engine edits faces, not individual edges, so those can't run yet. |
| 2 | **Solid Inspector²** (Thomas Thomassen; MIT, source on GitHub) | 2.5.2 | **works** (checked) | Loads with its bootstrap, error reporter and settings; registers its menu item and toolbar; clicking a group or component inspects it, and a cube with its top missing is reported as a hole / surface border, through its panel's `list_errors` call. Not working yet: the panel isn't shown in the app (its HTML and calls are recorded), the red legends and the camera zoom are drawn to a view that draws nothing, and "fix" buttons that edit single edges. |
| 3 | **Bezier Spline** (Fredo6) | — | **calls covered** | Fredo6's extensions ship encrypted (`.rbe`) with a shared library (LibFredo6); we can't read them, so the real one is expected not to load. The fixture is a click-to-add-control-points tool with a segment count in the VCB and Return to finish, drawing the curve with `add_curve`. |
| 4 | **s4u Make Face** (Sketchup4u) | — | **calls covered** | Paid after a trial (SketchUp's own list prices it), so it isn't tested. The fixture reads the faces of the model, builds a face from its outline in a group, and sweeps a profile along an outline with Follow Me. |
| 5 | **Add Height from Datum** (SketchUcation) | — | **calls covered** | The download needs a SketchUcation login, so it isn't tested. The fixture keeps a datum as a model attribute, takes it from an input box, and labels a clicked point with `add_text`. A datum set in one run is not kept for the next yet. |
| 6 | FredoScale (Fredo6) | — | not tested yet | Encrypted like Bezier Spline; expected not to load. |
| 7 | Round Corner (Fredo6) | — | not tested yet | As above. |
| 8 | JointPushPull Interactive (Fredo6) | — | not tested yet | As above. |
| 9 | Curviloft (Fredo6) | — | not tested yet | As above. |
| 10 | TT_Lib² (Thomas Thomassen) | — | not tested yet | Needed by CleanUp³ and most of Thomas's older extensions. |
| 11 | Material Replacer (Thomas Thomassen) | — | not tested yet | Needs TT_Lib². |
| 12 | Selection Toys (Thomas Thomassen) | — | not tested yet | Needs TT_Lib²; selects by reading the model, which now works for faces. |
| 13 | Quad Face Tools (Thomas Thomassen) | — | not tested yet | Needs TT_Lib²; edits edges. |
| 14 | Eneroth Upright Extruder (Eneroth3) | — | not tested yet | A mouse tool plus Follow Me; the pieces it needs exist now. |
| 15 | Eneroth Auto Weld (Eneroth3) | — | not tested yet | Uses an entities observer. |
| 16 | Eneroth Solid Tools (Eneroth3) | — | not tested yet | SketchCraft has its own Solid command. |
| 17 | SketchUp STL (SketchUp Team) | — | not tested yet | Writes files; scripts can't. Use File › Export instead. |
| 18 | Instant Road (Didier Bur) | — | not tested yet | |
| 19 | Flowify (Didier Bur) | — | not tested yet | |
| 20 | 1001bit Tools, free edition (Fredo6) | — | not tested yet | Encrypted like the other Fredo6 extensions. |

## Running the real ones yourself

The tests never contain other people's code. To run them against a copy you downloaded, put each in its own folder
(treat downloads as untrusted: the sandbox is what keeps them from harming anything, but don't unpack them in a
folder you care about) and name the folder:

```sh
cd app
SC_EXT_SOLID_INSPECTOR2=/path/to/solid-inspector/src npx vitest run tests/rubycompat.test.ts
```

Solid Inspector² is at <https://github.com/thomthom/solid-inspector> (MIT) and CleanUp³ at
<https://github.com/thomthom/cleanup> (MIT). Without the variable the test is skipped.

## What the Ruby layer can do now

Added for this work, on top of what the [Ruby guide](../dev/sketchup-ruby.md) lists:

- **`Sketchup::Tool`** with `activate`, `deactivate`, `suspend`, `resume`, `onMouseMove`, `onLButtonDown`, `onLButtonUp`,
  `onLButtonDoubleClick`, `onRButtonDown`, `onKeyDown`, `onKeyUp`, `onUserText`, `onReturn`, `onCancel` and `draw`;
  `model.select_tool`, `model.tools.push_tool` / `pop_tool`; the view (`inputpoint`, `pick_helper`, `invalidate`,
  the drawing calls, which draw nothing yet), `Sketchup::InputPoint`, `Sketchup::PickHelper`, the status bar and VCB.
- **Reading the model**: the faces (and their edges and vertices, joined up) and groups as they were when the script
  started, `entities.grep`, `face.edges`, `edge.faces`, `all_connected`, `classify_point`. Changes to what's there go
  by the real id (`face.pushpull` on an existing face pulls that face).
- **Observers**: selection, entities, model, tools and app observers, called for selection changes, new entities and
  committed operations.
- **`UI::HtmlDialog` and `UI::WebDialog`**: HTML, callbacks and `execute_script` are recorded and callbacks are
  called by events; **`UI.start_timer`** (fired by a tick event); `UI.toolbar`, command tooltips and icons.
- **Follow Me** (`face.followme(edges or points)`) and **textures** (`material.texture = [data_url, width, height]`).
- A few library pieces extensions lean on: `JSON`, `String#unpack`, `Array#pack`, `Set` operations, `Sketchup.read_default`.

What is **not** there yet: the app doesn't show panels or feed a Ruby tool the mouse (the tests and the API do), so
a Ruby tool started from the menu activates and ends in the same moment; scripts can't read image or other files, so
textures come as `data:` URLs; edges of existing geometry can't be erased or edited; model attributes and
`write_default` values aren't kept between runs; `Sketchup::Overlay` and `view.draw` output.
