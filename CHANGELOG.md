# Changelog

What changed in each SketchCraft release, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.3.9] - 2026-11-20

### Added
- **A SketchUp-like Classic look, and a right-click that always works.** New models start in a **Classic** style: a light
  blue sky over a grey-green ground, thick dark profiles on outlines, thin dark edges elsewhere, no line weights, off-white
  front faces and blue-grey backs. It is the first entry in **Style > Preset** (the earlier look is **Default**, and every
  other preset is still there); the look you pick is remembered, and a saved scene keeps its own style.
  Right-clicking now opens the context menu even when the mouse jitters, a touchpad two-finger tap raises only a
  context-menu event, or the pointer is captured by the view; a real right-drag still pans. **Shift+F10** and the Menu key
  open it for the current selection. The menu on a selection now always says **Make group** (it used to say
  "Group these (nest them)" for groups).
- **Texture painting.** **Tools > Texture Paint** paints straight onto a model's texture picture. Select faces (or a group),
  make a **New paint texture** (512, 1024 or 2048 pixels, starting from a flat colour or from what the faces look like now;
  if they have no UVs, or UVs that overlap, one click unwraps them first) and drag over the model: the brush reaches every
  part of the picture it touches in 3D, even across the cuts between UV islands, and the picture updates as you drag.
  Size, hardness and opacity are in the panel; **[** and **]** change the size, **Alt** erases back to the base colour,
  **Cmd / Ctrl-click** picks up the colour under the cursor, **Fill** floods the selection. The UV editor has a **Paint**
  mode that paints the flat picture the same way. A stroke is one Undo step. The picture is saved inside the model like any
  texture (older versions open it fine), goes into glTF / GLB and is written beside OBJ, FBX and Collada files;
  **File > Export texture…** saves it as a PNG. Agents use `texture_paint_new`, `texture_paint_stroke`,
  `texture_paint_status` and `texture_pixels`.
- **UV editor: unwrap, pack and edit a model's UVs.** **Tools > UV editor** (or **Window > UV editor**) opens a floating
  2D view of the selected faces with the material's picture under their UVs. Mark seams on edges (**Mark seam** on edges
  you chose in the 3D view or here, or **Auto seams** for every hard edge), press **Unwrap** and the model is cut along
  the seams, each piece flattened without stretching it (and cut further where a surface cannot be flattened whole) and
  all pieces packed into the square. Click an island to select its faces in the 3D view (and the other way round), drag
  islands, points or edges, move / turn / scale with **G / R / S** or the arrow keys, flip and turn 90, **Pin** points and
  unwrap again to keep them where they are, **Pack** again after moving things, **Reset**. The **Checker** toggle shows a
  checker here and on the model so stretching is easy to see. Every edit is one Undo step. Seams show red in the 3D view.
  Seams and UVs are saved in the model (older versions open such a file read-only). Agents use `uv_seam`, `uv_unwrap`,
  `uv_pack`, `uv_set`, `uv_transform` and `uv_info`.
- **Lightmap UVs.** **Lightmap UVs** in the UV editor makes a second UV set with no overlaps, packed into the square, for
  baked light. It is never drawn on the model; glTF / GLB carries it as `TEXCOORD_1` and FBX as a second UV layer
  ("LightmapUV"). Agents use `uv_lightmap`.
- **Vertex painting.** **Tools > Vertex Paint** (or search for it) lays colour straight onto the corners of faces with a
  brush: drag over the model and every corner inside the brush circle moves toward the colour you picked, fading out
  toward the circle's edge. **[** and **]** change the brush size, the panel has Strength and the edge (soft, linear,
  hard) and Fill / Clear for the selection, and **Alt** brushes the colour away. A stroke is one Undo step. The colours
  blend smoothly across each face and multiply with its material, so a plain white face shows the colour as painted;
  **View > Vertex colours** hides them. They are saved in the model (a model that has them opens read-only in older
  versions, so they are not lost) and go into glTF / GLB as vertex colours, into FBX as a colour layer, and into renders.
  Agents use `vertex_paint`, `vertex_paint_clear` and `vertex_colors`.
- **Position Texture: move, rotate, scale and skew a picture on a face.** Right-click a face that has a picture and choose
  **Texture › Position** (or Tools › Position texture…). The whole picture shows see-through over the face with four pins:
  **red** moves it, **green** scales and turns it around the red pin, **blue** stretches or leans its height, **yellow**
  skews it; dragging the picture itself moves it too. The face follows live; **Enter** (or a click on empty space) keeps it
  as one Undo step, **Esc** takes it back. Right-click inside for Flip left/right, Flip up/down, Rotate 90/180/270 and
  Reset. It works on a plain, a painted-through and a projected face alike. Agents use `texture_position`.
- **Painted-through pictures are now marked in the file.** A model with a picture carried across faces by paint-through is
  saved so that older versions open it read-only instead of silently dropping the placement on their next save.
- **Export to FBX.** File > Export > FBX writes a model for Unity, Unreal, Blender and Maya: each group and component is
  its own object, nested as in your model and keeping its name, with normals, texture coordinates, materials and the
  pictures used as textures (saved in a folder beside the file). Metres, Y up. Agents use `export_fbx`.
- **Project texture: planar, box, cylinder and sphere.** Select faces, a group or a component and choose **Tools ›
  Project texture…** (or right-click, or search for it) to lay a picture on them. A small panel shows the result live
  while you pick the shape, the picture, its size and turn, and whether it fits the selection once; Apply keeps it as one
  Undo step. A cylinder wraps once around like a label on a can (caps stay flat), a sphere like a map on a globe, a box
  gives each face the side of the box that fits it. It is saved in the model, shows in renders, and goes into glTF, OBJ
  and Collada. A model that uses it is marked so older versions open it read-only instead of dropping it. Agents can use
  `texture_project`.
- **A texturing test set in the Library.** Six small models made to try textures on: the beaver, dice, a drinks can, a
  globe on a stand, a wooden crate of separate planks, and a soccer ball with 12 pentagons and 20 hexagons. Open the
  Library's **Texturing test set** section and click one to place it at its real size (Italian names included). The
  coming texturing features are built and tested on them.
- **Paint-through: a texture that carries on across faces.** Hold **Ctrl** (**Option** on a Mac) with the Paint Bucket
  to paint every touching face that has the same material, and a texture now runs on across the corners and folds as
  one sheet (same tile size, lined up on each edge) instead of restarting on every face. **Shift** paints every
  face with the same material, and **Ctrl+Shift** every matching face on the whole connected object. One click is one
  undo, the step hint beside the model lists the keys, and agents get the same reach with `scope` on the paint command.
- **The Vertical layout has one sidebar with four full-height tabs.** Outliner, Library and Views are joined by
  **Properties** (Document, Style, Paint, Finish and the selection details), which used to sit in a fixed panel below
  and squeeze everything. Now every tab uses the whole height, so there is less scrolling. A dot on Properties shows
  when something is selected (you stay on your tab), the arrow keys move between the tabs, and your last tab is
  remembered. Classic and Floating are unchanged.
- **A real menu bar: File, Edit, View, Camera, Draw, Tools, Window and Help.** It replaces the single File drop-down
  next to the logo. File holds New, Open, Save, Save As, Restore from backup, Import, Export, Render, blueprints,
  drawing sheets, Layout, Timeline, locations, 3D text, reports and Plugins; Edit has Undo and Redo, Cut, Copy, Paste
  (and Paste in place), Delete, Select all and none, Make group and component, Explode, Hide and Unhide; View has the
  workspaces, face styles, backgrounds, Shadows, X-Ray, Back edges and the other style switches, the section cut, the
  panels, the toolbar layout, Themes and Language; Camera has the standard views, Perspective and Parallel, Zoom extents
  and the Orbit, Pan, Walk and other camera tools; Draw and Tools list the drawing and editing tools; Window shows the
  Outliner, Library, Views and the right panel and opens the shortcut editor and the plugin manager; Help has the Learn
  center, Keyboard shortcuts, Search commands, Check for updates (desktop app), Report a bug and About. Each item shows
  its keyboard shortcut (the one you have set), and items that don't apply right now are greyed out. Click a name to
  open a menu, point at another name to move between menus, and use the keyboard: F10 (or a tap on Alt on Windows and
  Linux) focuses the bar, the arrow keys move and open submenus, Enter runs an item and Esc closes. Nothing that was in
  the old File menu is gone: Keyboard shortcuts and Check for updates moved to Help, Customize shortcuts to Window,
  Language to View, and the version to About. On narrow windows the workspace switch is hidden (View > Workspaces has
  the same choices). Italian wording for the menus and About is welcome from Italian speakers (Kad26 has been asked to
  review it).
- **Help > About SketchCraft.** The app's name and version, then "Always free. Never freemium. Never paid." and "It's
  a promise.", links to the source on GitHub, the MIT license and the changelog, and a **Thank you** to everyone who has
  written code for SketchCraft. That list is made when the app is built, from the authors in the project's git history
  (bots are left out), so it never needs updating by hand.
- **Fixtures & furniture for your rooms.** The Library has a new "Fixtures & furniture" section with 26 ready-made pieces
  at common US/EU sizes, in five rooms: Bath (toilet, 0.6 m and 1.2 m vanities with sinks, bathtub, shower stall, towel
  bar), Kitchen (base cabinet, sink base, wall cabinet, pantry, fridge, range, dishwasher, island), Bedroom (queen, king
  and twin beds, nightstand, dresser), Living & dining (3-seat sofa, armchair, coffee table, TV stand, dining table with
  four chairs) and Laundry (washer, dryer). Each shows its name and size (metric and inches) when you hover it, shows a
  thumbnail, and is found by searching for its name or its room. Each stands on the floor with the middle of its back edge
  at its origin, so it can sit against a wall. While you place one, bring the cursor within about 40 cm of a wall and it
  turns to face into the room and slides back flush against the wall ("Against the wall"); hold Shift to place it freely.
  One click is one Undo step. Italian wording is included; a review by Kad26 is welcome.
- **Room mode: draw a room by dragging it out, like in The Sims.** Pick **Room** in the Build panel (or on the Arch
  toolbar), press on the ground, drag a rectangle and let go: you get four walls of the usual height and thickness and a
  floor, snapped to a 0.1 m grid (6 in in feet and inches). While you drag, the inside width, depth and area show in
  your units (m² or ft²). Each room is a group named Room 1, Room 2 and so on (rename it in the Room options) on the
  "Rooms" tag, with its walls on the "Walls" tag, so it saves, exports and undoes like anything else, and one Undo
  removes a whole room. A room dragged against another shares that wall instead of doubling it, a side within one wall
  thickness of a neighbour's wall snaps onto it, and a room that would overlap another is refused with a message and
  nothing changes. While Room mode is on, every room shows its name and area, and they update as walls move. Dragging a
  wall in Room mode is Wall pull, so the rooms on both sides grow and shrink together; clicking a wall adds a door or a
  window (your choice in the Room options). **Walls down** shows the walls low so you can see inside; it only changes
  the view, never the model. Agents can use `room_rect`, `room_preview` and `rooms`. Italian wording for the new tool
  is welcome from Italian speakers.
- **Drag a room over another room to combine with it, or hold Alt (Option) to cut a notch out of it.** Rooms are no
  longer only rectangles. Drag a new room so it overlaps an existing one and the two become one room with an L-shaped
  (or T-shaped, or any square-cornered) outline: the walls inside the overlap disappear and walls run round the new
  outline. Hold Alt (Option) while you drag and the box is cut out of the room instead, leaving a notch. The preview
  shows the result, not the box you are dragging, the floating label says what will happen ("Combine with Kitchen",
  "Subtract from Kitchen") with the new area, and the step hint explains the Alt key. The room keeps its name, its other
  neighbours' walls are left alone, and doors and windows on walls that still exist are put back in the new walls (the
  ones whose wall is gone are removed, and you are told how many). A cut that would split the room in two or leave a
  hole in it is refused with a reason, and one Undo takes back the whole change. Room labels now sit inside L-shaped
  rooms instead of in the notch. Agents can pass `mode: "combine"` or `"subtract"` to `room_rect` and `room_preview`.
- **Pull a whole wall, like in The Sims.** The new **Pull wall** tool (in the Build panel) lights up a wall as one piece
  when you point at it, and drags it sideways; type a distance and press Enter for an exact pull, or Esc to cancel. The
  wall keeps its thickness, the walls joined to it at corners and T-junctions stretch or shrink to stay joined (so a
  room grows or shrinks), and the doors and windows in the wall go with it. Doors and windows in the walls that stretch
  stay where they are, and if one would no longer fit the pull is refused with a message and nothing changes. It works on
  walls made with the Wall tool and on plain box-shaped walls in other models; walls that are sloped, curved, or carry
  on in a straight line past their end are refused with a reason. The whole pull is one Undo step. Agents can use it
  as `wall_pull`. Italian wording for the new tool is welcome from Italian speakers.

### Fixed
- On Linux, right-dragging to pan no longer opens the context menu, and clicking a submenu's name after hovering it no longer shuts it again.
- **The top bar fits a 1366-pixel laptop screen.** On wider system fonts (Linux) the menus ran under the workspace
  switch at that width. The product name now steps aside below 1400 pixels, and a long file name is shortened with an
  ellipsis before it can crowd the menus. Wider windows look as before.
- Security: hardened limits on texture painting, UV editing and texture export (WildLeopard).

## [0.3.8] - 2026-11-18

### Added
- **Big drawing sets use far less memory and redraw faster.** The 52-page A1 permit set (about 200,000 faces) missed two
  of its budgets the first time it was measured: it peaked at 1.94 GB of memory (budget 1.5 GB) and its slowest drawing
  took 1.06 s to redraw after a wall moved (budget 1 s). Drawings kept for the pages now hold their plain lines packed
  (about a fifth of the memory, 92 MB instead of 540 MB for the set), and the hidden-line views skip the work for
  anything certainly hidden behind a big wall. The drawings, PDF, SVG and DXF files come out exactly the same as before.

### Fixed
- **Changes the app groups together now undo in one step.** When the app sent several changes as one group (reordering
  the intersection priorities of several tags, for example), the first change closed the group, so Undo had to be
  pressed once per change. A group of changes from one source is now one Undo and one Redo. Groups stay apart by
  source: an agent's group, the app's own group and a plugin's run never mix, a group that is left open is closed on its
  own, and opening a group inside a group or closing one that isn't open now says so. Agents can group their commands
  too, so a whole AI answer can be taken back with a single Undo while your own edits stay their own steps.
- **SketchUp 2013 to 2020 files with curves, pictures or text labels import again.** A SketchUp 2016 model (the
  "SpeedyBee" file from #1) was refused as damaged because SketchCraft stopped at the control points a Bézier curve
  keeps in its attributes; it also could not read the model's pictures (now placed like components) or its text labels
  (found and counted among the annotations left out, not placed). Parts that were turned (a quarter turn about an
  axis, say) in these files also came in turned the wrong way, so they sat scattered; they now sit where SketchUp has
  them, as the same fix already did for newer files. On that file the import now gives about 5,000 faces, 25,200 edges
  and 377 placements, against 4,990, 25,140 and 377 in SketchUp 2026's re-save. Dimensions in large classic files
  (over 32,767 objects) are read correctly too. (part of #70)
- Security: hardened limits on sheets, tables, rendering and imported text (WildLeopard).

### Added
- **Tables and text blocks on sheets.** A Table tool (B) in Layout: click for a table, or drag the size you want, and
  type straight into its cells (Tab goes across, Enter goes down, Alt+Enter starts a new line in a cell); double-click
  any cell to change it. Paste rows copied from a spreadsheet (or comma-separated text, quotes and all) onto the page
  for a new table, or into a picked table from the cell you clicked; it grows to fit. The inspector sets a title, adds
  and removes rows and columns, the header rows and a bold first column, the borders (grid, rules, outline or none),
  the text height, each column's width and the alignment. "Drawing index" fills a table with every sheet's number and
  name and keeps it up to date. Drag across with the Text tool for a text block whose paragraphs wrap to its width,
  left, centred or right aligned. Tables and text blocks print in the PDF (each table a real table for screen
  readers), SVG and DXF, are saved with the model, and every edit is one Undo. A 200-row schedule or a page of notes
  stays quick. Agents can make them too (`sheet_table`, `sheet_table_paste`, `sheet_text` with `width_mm`).
  (part of #7)
- **Lay out a sheet entirely by mouse.** Layout has a tool bar down the left of the paper: Select, Viewport, Line,
  Polyline, Rectangle, Arc, Text, Dimension, Trim, Extend, Offset and Delete, each with a key (V, W, L, P, R, A, T, D,
  X, E, O, K), its icon as the cursor, and a hint under the paper saying what to do next. Drag a frame with Viewport
  and pick what it shows (a floor plan, an elevation, a 3D view or a saved scene). With Select, drag a view to move it
  and its eight handles to resize it; arrow keys nudge it (Shift for a grid step), and the scale is set on the right.
  Everything snaps, with a marker saying to what: the paper grid (5 mm, or your own size; it can be turned off), the
  page's edges, margin and middle, the edges and middles of other views and the title block, the ends and middles of
  lines, and the model's corners and middles. Dimensions attach to the model: click its points, keep clicking to make
  a chain, Shift for a baseline set, and Enter or a double-click puts the line where the mouse is (Auto, Horizontal,
  Vertical or Aligned at the top). Text: click and type; double-click text to change it. Trim shows in red what a click
  would cut away (Shift extends instead), Extend shows where a line would run to, and Offset shows the copy and its
  distance (or type one). Double-click the title block to type the project or the page's name on the page. Escape
  cancels a step, every gesture is one Undo step, and it is all saved in the model. Everything on the page can be
  reached with Tab and the arrow keys, with a label for screen readers. Agents get `sheet_text` (paper text) and
  `preview: true` on `sheet_trim` and `sheet_extend`. (part of #9)
- **A Layout workspace for drawing sheets.** A fifth workspace, Layout, beside Model, Arch, Design and Previz. Pages
  are on the left: add, duplicate, rename (double-click or F2) and delete them, and drag them into order (or the
  arrows, or Alt+Up and Alt+Down). Each is one Undo step. The page you pick is in the middle, drawn on paper exactly
  as the PDF draws it: scroll to zoom, drag to pan, Fit to see the whole page. Click a view on the page to change it on
  the right: its caption, what it shows, its scale (1:50, or imperial like 1/4"=1'-0"), and an **opacity slider**
  from 0 to 100%, for tracing one view over another (#6). With no view picked, the right side has the page: its name,
  number, paper size, the title block's project and notes, and "Add view" to put a plan, an elevation or a 3D view
  on it. Your 3D model stays open underneath while you're there, so switching back is instant and the camera,
  selection and tool are as you left them. File > Open in Layout, the Arch strip's Layout button and the Drawing
  sheets dialog take you there. Agents get `sheet_ir` (a page's drawing as data), `sheet_duplicate` and
  `sheet_order`. (part of #63)
- **Cut intersection priority per tag: the wall wins where it runs into the slab.** When the cut surfaces of two tags
  overlap in a plan or a section (a wall into a slab, insulation inside a wall), the tag with the higher priority
  keeps the overlap and the other stops at its edge, with no seam. Cuts of one tag, or of tags with the same priority,
  join into one shape. In the Outliner, under Tags, "Intersection priority" lists the tags with the winner on top:
  drag to reorder, use the arrows (or Alt+Up and Alt+Down on a row), or type a number from 0 to 999. It shows on
  screen and on sheets (PDF, SVG, DXF), and agents set it with `tag_style {cut: {priority}}`. With every priority at
  0, which is the default, drawings look as they did. Suggested by a Reddit user. (part of #88)
- **Ask an agent to draw the sheets: lay out a permit set, dimension the plans, hatch, check.** `layout_preset`
  makes a permit set of ordinary saved sheets in one Undo step: a cover with the sheet index and a 3D view, a plan
  per floor, the four elevations, a section through the stairs (when there are stairs) and the door and window
  schedule, numbered G-001, A-101, A-201, A-301 and A-601, on the paper and at the scale you ask for. It refuses an
  empty model. `dimension_auto` puts attached chain dimensions on a plan, worked out the way the blueprint's are:
  the overall width and depth, a chain through both sides of every door and window, every wall face across the
  building, and column centres and guide points as grids. They follow the walls when you move them, a re-run never
  doubles them, and dimensions it didn't make are never touched. `viewport_points` lists what a drawing can be
  dimensioned to, with where each point is on the paper, and `sheet_render` (or the agent's render tool with a
  sheet) shows a sheet as a picture so an agent can look at its own work. The agent help now has a short drafting
  recipe: inspect, lay out, dimension, hatch by tag, render to check, fix. Each step is one Undo. (part of #10)
- **Hatches on screen at their true weight.** When a section plane cuts a tagged wall, its hatch is now painted by
  the graphics card: lines as thick on screen as the hatch's pen weight says (0.35 mm draws wider than 0.13 mm, 6
  pixels per paper millimetre, as edges and cut outlines are), with its dashes and dots, and no limit on how many
  lines a big or finely hatched cut can have (it used to stop at 60,000 lines and fill the rest with a flat tint).
  A pattern with more than 16 line families falls back to the old lines. (part of #8)
- **Big drawing sets stay fast.** SketchCraft now keeps each viewport's drawing once it has been drawn, and only
  redraws the ones a change can reach: move one wall in a 52-page A1 set and the plans, elevations and sections that
  show it are redrawn (30 of its 70 viewports in our test set), while everything else, and every page whose only
  change is a dimension, a line or a text, comes straight from what was kept. Hatches are kept per region too, so a
  redrawn plan reuses the hatching of the walls that didn't move. Nothing is drawn when a file opens; pages are
  drawn when you look at them or export. The pages you looked at last are redrawn in the background after a change,
  using at most two threads, and a result is thrown away if the model changed while it was being drawn, so nothing
  out of date is ever shown or exported. Memory for kept drawings is capped (1 GB), least recently used first. A
  set can now have up to 200 pages (it was 50). `drafting_status` tells the editor, agents and plugins how the kept
  drawings stand. A benchmark of a 52-page A1 permit set (about 200,000 faces, 2,000 dimensions, 400 text blocks,
  hatches on 8 tags) checks the timings against a budget on every release candidate and nightly. A saved viewport is
  now drawn exactly where its anchor says (it was re-centred on the building on every export, while its dimensions
  used the anchor). (part of #8)
- **Line work on sheets: draw, trim, extend and offset.** A sheet can hold plain lines, polylines, arcs and
  rectangles (`sheet_line`: a pen weight in paper millimetres, a colour, optional dashes), drawn in the PDF, SVG and
  DXF. `sheet_trim` cuts away the span you pick between the nearest places other things cross it: the other line work
  on the sheet, the lines a viewport draws of your model, and the edges of viewports. A span in the middle leaves two
  pieces, one at the end leaves one, and a rectangle opens into a polyline. The model's own lines are only ever
  boundaries, never cut. `sheet_extend` runs the end nearest your pick on, along the line (or round the circle for an
  arc), to the next boundary. `sheet_offset` makes a parallel copy: lines move over, polylines get mitred corners,
  arcs get a bigger or smaller radius, rectangles grow or shrink. When there is nothing to trim to or extend to, or a
  distance would collapse the shape, the command says so and changes nothing. Each one is a single Undo step, and
  agents and plugins can use them. There is no mouse editor for these yet (that is the sheet editor, #9); this is the
  engine and the commands it will use. A chain of dimensions was already one command: `sheet_dim` with more than two
  points measures every span in one go (or every span from the first point, as a baseline set), and is now written up
  in the agent help. (part of #19)
- **Dimensions from SketchUp files stay attached.** A dimension imported from a SketchUp file now holds on to the
  corners or edges it measures, so when you move a wall the dimension moves with it and shows the new length; undo
  puts both back in one step. An end that sits on nothing (within a thousandth of an inch, SketchUp's own tolerance)
  keeps its fixed point, and the import tells you how many dimensions attached and how many didn't. SketchUp text
  labels and screen texts are now read fully: they are still left out of the model (no sample file shows where a
  label's text sits against its leader), but the import counts them and lists their words so you can add them back.
  A file that holds only a screen text opens instead of being refused. Models with attached dimensions save in the
  0.3.8 file format (0.3.7 opens them read-only); dimensions you drew yourself, and older files, are unchanged.
  (part of #59)
- **Cut fill per tag.** Where a plan or section cuts something, each tag now decides how its cut looks: the usual
  dark fill, a solid colour of your choice, its hatch, or nothing at all (just the outline), with its own outline
  thickness. A section through walls, a slab and furniture on different tags shows each its own way, on drawing sheets
  (PDF, SVG and DXF) and on screen with a section plane. In the Outliner's Tags list, the new Cut button next to Hatch
  shows a swatch of the fill and opens the choices, each with a hint. A cut takes the tag of the innermost tagged
  group or component around it, so a tagged part inside a component (the glass in a desk) keeps its own fill. It is
  saved with the model and undoes in one step; agents and plugins can set it with `tag_style {cut}`. (part of #5)
- **Dimensions follow a group's copies.** When you group faces that share corners with other geometry, the group gets
  its own copy of those corners. A dimension on a sheet now moves onto the copies when everything it measures went
  into the group, so it stays with the group when you move it; a dimension that also measures something left outside
  stays where it was. It is one undo step with the group, and redo brings it back. (part of #18)
- **Hatch patterns per tag.** In the Outliner's Tags list, each tag has a Hatch button: pick a pattern and its walls
  are hatched wherever a plan or section cuts them, on drawing sheets (PDF, SVG and DXF) and on screen with a section
  plane. Set the hatch's colour, line thickness, scale and angle, and whether its size is fixed on paper or real size
  in the model. Eleven patterns come built in (diagonal, cross, steel, grid, dots, brick, concrete, earth,
  insulation, wood, sand), drawn by us and free to use (CC0), and **Import .pat…** loads AutoCAD-format `.pat` files,
  which are kept in the model so it opens the same anywhere. In a DXF the hatch stays a real hatch your CAD program can
  edit. A pattern too fine for its region prints as a light tint, and the export says so. Agents and plugins can do
  the same with `tag_style {hatch}`, `hatch_pattern` and `hatch_patterns`. (part of #4)
- **A viewport on a sheet can be faded.** Give a viewport an opacity from 0 to 1 (`viewport_edit` with `opacity`, or when
  you add it) and, in the PDF, it prints as one see-through drawing: lines that cross inside it don't darken each
  other, and viewports stacked on one another blend in the order they are stacked. SVG keeps the fade too. DXF has no
  opacity, so those viewports are drawn at full strength and the export result says so in `warnings`. A faded 3D
  view over a vector plan ("hybrid") isn't drawn yet. (part of #6)
- **Dimensions come in from SketchUp files.** Importing a SketchUp 6 to 2020 file now brings its linear dimensions
  along as dimensions on the model, measured between the same two points and drawn at the same offset; a dimension
  whose text was typed over (not the measured length) also gets a text label. Radial dimensions, free text labels with
  leaders, and the annotations in 2021 and later files still aren't imported (the import result says how many
  dimension records it had to leave out).
- **Line weight and line style per tag and per edge.** In the Outliner's Tags list, each tag has a weight menu
  (0.13 to 2 mm) and a style menu (solid, dashed, hidden, center). Select an edge and Entity Info has the same two
  menus for it; an edge's own setting beats its tag's, which beats the default. They show in the model view (with a
  new "Line weights" switch in Style) and print on sheets as real pen widths in paper millimetres, the same at
  any scale, in PDF, SVG and DXF (DXF gets true lineweights and linetypes). New commands `tag_style` and
  `edge_style` (agents and plugins may use them); they save in the model file, undo one step each, and a model that
  uses them opens read-only in older versions instead of losing them. (part of #3)
- **Customizable keyboard shortcuts, with SketchUp import.** Press ? › Customize… (or
  File › Customize shortcuts…): search a command, click its key and press a new one,
  swap or clear a key another command has, reset one or all. Tools, menu commands and
  plugin commands can all have keys. A **SketchUp** preset gives SketchUp's own defaults;
  **Import** reads a shortcuts file exported from SketchUp (and lists what it could not
  match), and your own keys export and import as JSON. Keys are saved per user and
  checked on load, so a bad saved value falls back to the defaults. Esc, Enter, Tab in
  SubD, the arrows and the measurement digits can't be taken. The ? sheet, tool tips,
  the command search and the Learn lessons show your own keys. (#60)
- **Drawing sheets are saved in the model file** (design step 1 of 0.3.8 "Drafting": the data only; drawing them
  and editing them come next). A model can hold sheets (page records: name, number, paper size, order), and
  viewports and other things on them as flat items, so Undo works one item at a time, even across another
  program's save. A viewport keeps its own anchor, scale, rotation and crop; nothing re-fits it when the model changes.
  New commands: `sheet_add`, `sheet_edit`, `sheet_delete`, `viewport_add`, `viewport_edit`, `sheet_item_delete`,
  `sheets_info`. `sheets` now creates saved sheets from its page description, and `export_sheets` writes the PDF
  (it still takes the old one-shot description). A model with sheets is saved as format 2 and
  needs `sheets`, so SketchCraft 0.3.7 opens it read-only instead of dropping them; a model without sheets is still
  format 1. Kinds of paper item a newer version adds are kept as they are and written back. Agents and plugins may
  edit sheets (at most 120 sheets, and 5,000 items in one command); only the person may issue a set. (#61)
- **Dimensions on sheets stay attached to the model.** A dimension on a sheet (`sheet_dim`) is attached to points of
  your model, not to fixed numbers: move a wall, refresh the sheet, and every dimension on it follows and shows the new
  length. A chain of dimensions follows every point in it; a baseline set measures each span from the first point.
  Splitting an edge keeps a dimension on the right place, and turning geometry into a component (or exploding one)
  carries its dimensions with it. Delete the geometry and the dimension shows **broken**: magenta, a "!" where it was,
  and "?" for the length (`sheets_info` lists the broken ones and the points near where they were). It is never moved
  or re-attached by itself; `sheet_reattach` attaches a broken one to a point you pick, or every broken one that has
  exactly one point where it was. Undo restores all of it in one step. Viewports keep their crop and scale however the
  model changes. (#18)
- **Crisp vector drawings.** Drawing sheets and blueprints are built from one paper-space
  display list that feeds the PDF, SVG and DXF writers. Elevations, sections and parallel 3D
  views are now vector line work with hidden lines removed (they were pictures), and the PDFs
  embed their fonts, so accents and other non-ASCII text work. Line weights are pens in paper
  millimeters (ISO 0.13 to 0.5), not scaled with the drawing. Viewports can show what's
  hidden behind other things, dashed. `export_sheets` writes one page as `.svg` or `.dxf` (AC1015,
  with lineweights) as well as a PDF. (#62)
- **More paper and imperial scales.** A1, A0, ARCH A to E and ANSI C to E sheets, and scales like
  `1/4"=1'-0"`, `1/8"=1'-0"` or `1"=20'`. (#62)

### Changed
- **A drawing stays where it was made.** A viewport can carry its own anchor, and drawings
  made without one are fitted to the building, not its environment, so adding a tree no longer
  moves every drawing. (#62)

### Fixed
- **The Vertical layout's tool bar keeps every tool on its background.** When the tools didn't fit one
  column, the second column spilled outside the bar with no background behind it, over the model. It's now
  laid out like Photoshop's tool bar: one column when there's room, two side by side when not (three or four
  on a short window), each group of tools in rows, and the bar exactly as wide as its columns. The view bar
  and the quick-action strip move over to stay clear of it.

## [0.3.7] - 2026-11-17

### Added
- **Subdivide from the app.** Right-click a group or component (or Shift+S, or the
  Library's Smooth shapes) › **Subdivide…** turns it into a smooth surface and opens
  SubD mode to shape its cage (Tab to shape, Enter when done). **SubD box** starts from
  a cube cage. There's a Learn lesson for it. Before, a subdivided object couldn't be
  shaped at all. (#56)
- **A Vertical tools layout** (Themes › Layout): tools down the left in two columns,
  the Outliner and properties on the right. Classic and Floating stay. Contributed by
  Kad26. (#46)
- **Files from a newer SketchCraft open read-only, so an older copy can never damage
  them.** Model files now carry a format number. If a file has a higher one, asks for
  features this version doesn't have, or holds data this version doesn't know, it opens
  for looking only: a bar says "Saved by a newer SketchCraft — update to edit",
  Save, Save As, the live copy and the backups never write over it, and an AI agent
  or plugin that tries to change it gets an error saying why. Restoring from a backup
  and placing library parts follow the same rule. Files from before this check open
  as always. (#57)

### Fixed
- **Trim keeps nested hardware.** Trimming a door leaf with a handle no longer deletes
  the handle, and a solid with reversed faces is refused instead of giving a wrong
  result. Contributed by Kad26. (#45)
- **The axis gizmo is fully visible in every layout.** In the Vertical layout it sat
  under the view bar; the tool hint, the SubD bar and the Value box no longer collide
  either. (#55)
- **Typed lengths like `10' 6 1/2"` stay one length.** Feet, a space and inches used to
  be read as two lengths (a width and a height).
- **Flip edge works on terrain** without opening its group first, like the other
  Sandbox tools.

### For contributors
- **A test suite that grows with the app**, run in CI on every pull request: browser
  tests that drive every tool with a real mouse and keyboard, frontend unit tests,
  screenshot comparisons in each layout and theme, and SketchUp's own test files
  (versions 6 to 22) with a corruption run. See CONTRIBUTING.md. (#49–#54)

## [0.3.6] - 2026-11-17

### Fixed
- **SketchUp files from every version open.** Files saved by SketchUp 2013 to
  2020 were refused ("not a SketchUp 2013+ archive"); they now import with their
  geometry, components and colours. Files from SketchUp 2021 and later (including
  SketchUp 27 and SketchUp for Web) now place all their components; some came in
  looking empty. Tested on SketchUp's own test models from versions 6 to 27, and on
  hundreds of damaged files, which are refused in under a second. (Textures in
  2017-2020 files come in as colours for now; that's planned for 0.3.8.) (#1)
- **The window drags by its title bar while it's focused** on macOS. Before, it
  only moved when another window was in front. (#2)
- **Push/Pull repeat** (double-click a face to push it the same distance again)
  never worked; it does now.

### Security
- **Final pre-release pass.**
  - Plugins and agents: a plugin run that changes your model now says so
    ("Plugin X changed 3 things", with an Undo button), the Undo and Redo buttons
    say when the step is from a plugin, and when you open SketchCraft after an AI
    agent saved changes you're told. The plugin install preview shows the same
    kind (JavaScript or Ruby) and Ruby menu items that the installer uses, and a
    plugin that is both is refused. Agents can no longer start programs (animate, IFC and DWG are
    yours alone), every helper program has a time limit and is stopped with
    everything it started, `--any-path` keeps the Backups, Plugins and hidden-file
    rules, and long or odd names in an agent's replies are cut and marked as data.
  - Your files: a model no longer records the folder a library part came from (older
    files lose it when opened), built-in doors, windows and figures can't be
    swapped by a component with the same name, and a library part is reused only if
    it is the same file contents.
  - Releases: the Rust version is pinned (CI is green again on newer Rust), each
    platform's installers are checked for their own names and file types before
    signing, only the Tauri CLI is installed next to the signing key, release
    files get build-provenance attestations, and a vendored Opal's unused network
    loader is removed. CI also checks that dependencies come from public registries only and
    that what ships has no high npm advisory.
  - Smaller: the plugin warning banner stays readable under any theme, a
    remembered Save path must end in `.sketchcraft`, the dev bridge deletes its token
    file when it stops, and the committed MCP setup holds no personal folder.

- **Hardening after a red-team review.** Your model is safer: it's saved
  atomically with rolling backups, a file that can't be read is kept aside
  instead of replaced, and a bad command can no longer corrupt it. Plugins
  (JavaScript and SketchUp Ruby) and AI agents are held to modeling commands, the
  plugin sandbox is tighter, installing a plugin shows what it is first, MCP
  agents work inside one folder, and the dev bridge needs a token. Updates and
  releases are checked more strictly, and releases publish SHA-256 checksums.
- **A second review closed the gaps in that.** Undo no longer steps back over
  work another client (Claude, a second window) saved, and a burst of New/Open
  can't push your last good backup out (backups are thinned instead of dropped,
  blank ones are skipped). A plugin run is now one Undo step. Opening a huge or
  booby-trapped DXF is refused quickly instead of freezing the app, and very
  long point lists or absurd stair/array counts are refused. AI agents write
  only the right kind of file with a new name (never over yours) and can't be
  steered out of their folder by an OBJ file. Plugin messages say which plugin
  they come from, and saved CSVs can't carry spreadsheet formulas.
- **A third review.** Backups are safer: only SketchCraft's own backups are
  counted, the newest of each time slot is kept, the copy taken just before a
  New, Open or big delete is kept for a day, and another model in the same folder
  can't thin yours. If another program saves while a plugin runs, or while your
  own save was failing, your changes are set aside in a `.unsaved-` file next to
  the model (with a warning that stays until you deal with it) instead of being
  lost. A plugin run is never re-pointed at someone else's model mid-run, and a
  run that is never closed is saved after a couple of minutes. AI agents can't
  write into Backups or Plugins through the library, and can't replace library or
  export files that already exist. Huge copies and arrays, dense DXF drawings,
  text with many outlines and grouping part of a big mesh are bounded or fast
  instead of freezing the app. The report CSV can't carry formulas, plugin tool
  previews can't crash the window, and a long Ruby script has a time limit.

- **A fourth review.** Backups can no longer be pushed out: forced backups are paced,
  your backups, an agent's and a plugin's each have their own small quota (with a byte
  limit too), a plugin run that wipes the model is backed up first, and the files set
  aside next to your model are only removed with a notice. Folder names such as
  `Backupſ` or `BACKUP~1` can't reach Backups or Plugins (they are compared by what
  they are on disk, not by spelling), and a name an agent has made is only "its own"
  while the file is still exactly what it wrote. A model can hold about 2 million
  points, edges and faces (was 8 million); a bigger one opens for viewing and cutting
  down instead of being refused. DXF wall layers and hatches, and 3D text, now stop at
  their time limit. A plugin run is closed on time even if the app is idle, and your
  own edits during a run are saved at once. The release workflow keeps the Apple
  secrets out of the job that compiles, and checks the file names it signs.

- **A fifth review.** Undo after another program saved now works on ordinary models
  (it used to refuse most of the time once you had rotated, scaled or imported
  anything), carries your history across a save without a pause even on huge models,
  and keeps a step it had to refuse so it can be tried again (press Undo again to skip
  it). It also refuses an undo that would leave another program's work pointing at a
  colour, group or part that the undo removed. Restore no longer reuses part numbers
  and refuses a damaged backup. Images, photos and scans count against a size limit
  (256 MB in a model), so a runaway tool can't make the model unopenable or push your
  backups out; your own big imports (a 215,000-triangle OBJ, a 354,000-line DXF, a
  300 x 300 terrain) work again. Backups keep one copy per hour for a day and per day
  for a month however many deletes you make, a few huge backups can't push out all the
  others, and Restore lists only the set-aside files SketchCraft itself made.
  Plugins can no longer use Undo and Redo; the release workflow asks the `release`
  reviewer before the Apple signing secrets can be read.

- **A sixth review.** One size limit now counts everything in a model (images, scans,
  names, notes, component attributes): about 512 MB of file, of which an AI agent or a
  plugin can take a model up to 160 MB, and no more than 32 MB at a time, so a runaway
  tool can no longer make your model unopenable. Models over the limits (made before
  this, or restored from a backup) now open for cutting down instead of being refused
  with a message you can't act on, and Restore is never blocked by the limit. A model
  SketchCraft saved always opens again (the open limit is 768 MB). If your model file
  can't be read when SketchCraft starts, nothing is saved over it until you restore a
  copy or start a new model. Opening a file (or a library part) whose part numbers
  were wrong no longer overwrites your drawing, and components are checked as
  thoroughly as the model. Huge pictures hidden in a small file are measured before
  they are opened, library thumbnails check the file size, and a face squashed to
  nothing can no longer stop the model drawing. Undo no longer lets a few millimetres
  of another program's edit be overwritten at survey-sized coordinates, and a Redo
  step survives a second save by another program. A recovery file whose note was lost
  is still listed in Restore ("not verified"). AI agents can no longer start the
  map and terrain download (Add location…); it stays yours. Very large models (over a
  million points, edges and faces) say so: each change takes a second or more.

- **A seventh review.** Models with parts inside parts (most SketchUp imports and
  library parts) open, restore and reload again: the sixth review's consistency check
  wrongly refused them. A model with a very long text pool (hundreds of thousands of
  named placements) opens for cutting down instead of being refused, and an AI agent
  can no longer use up the room your own names, notes and colours need. The file-size
  estimate counts escaped text, so a model SketchCraft saved always reopens. Formulas
  in Dynamic Components are worked out once each (a long chain no longer hangs the
  app), and an id counter at the top of its range is refused instead of overwriting
  your drawing. If SketchCraft can't read your file at start, saving comes back by
  itself when the file can be read again or is moved, or when you choose New or Open.
  An agent's `open` and library reads have their own smaller size limits, and a
  large import now shows the big-model notice straight away.
- **An eighth review.** A SketchUp file (or a saved model, or a library part) whose
  components contain themselves, nest more than 64 levels deep or multiply into
  millions of faces (a part placed 10 times inside a part placed 10 times, and so on)
  used to crash the app, and could crash it again on every start. They are now refused
  with a message, and a saved model like that is set aside like any damaged file
  instead of crashing the program on launch. Components can't be made, and SketchUp
  files can't be imported, while you are editing a component (close it first); that
  also fixes a bug where such an import could make a component contain itself. Opening
  a SketchUp 2021+ file is held to limits (archive size, pictures and time, at most 30
  seconds for any file), so a tiny booby-trapped file can't use up the computer's
  memory or freeze the app. A plugin tool that can't start now says so, and one that
  sends too many commands is stopped.
- **A second look at the eighth review.** Double-click now works everywhere (it did
  nothing in Windows and in browser builds): double-click finishes a wall, opens a
  group or component, selects a face with its edges (triple-click: everything
  connected), and repeats your last Push/Pull distance. Escape closes the Learn,
  Themes, Plugins and every other dialog, and the File and right-click menus. A
  damaged classic SketchUp file (2020 and earlier) is now refused in well under a
  second instead of freezing the app for up to 30 seconds. A saved setting that has
  gone bad can no longer stop the app starting. Placing a component while you are
  editing one says "Close the component you're editing first" and leaves Place mode.
  Another program that opens your model while you are editing a group no longer
  draws into that group, which could make Undo refuse to take back your own step.
  File > New while editing a component now really starts a new model. Pictures in a
  SketchUp file that can't be read, or are over 64 MB, are named in the import notice.

### Changed
- **Bigger pictures.** Textures and photos may be up to 16,384 px on a side and 100
  million pixels (48 MP phone photos and 8K textures are fine; it was 8,192 px and 40
  million). A picture that is too large is refused when you add it, with its size in the
  message, and a picture left out of a render, a SketchUp import or an opened file is
  named in a message instead of being dropped silently.

### Added
- **Delete or replace a material.** In Materials (Textures): each picture shows its
  size, **Replace picture…** puts another image on every face that uses it, **Delete**
  takes a material and its picture out of the model (its faces go back to the plain
  look, or to a material you choose), and **Remove unused materials** clears out what
  nothing uses. They are yours only: AI agents and plugins can't delete materials. This
  is also how you cut down a model that is over the size limits.
- **Restore from backup…** (File menu, or Shift+S): the model's earlier copies,
  newest first, with their time, size and what they were taken before ("before New",
  "before a plugin run"). Restore one as a single Undo step (the model it replaces is
  backed up first) or save it as a copy beside the model. Work SketchCraft set aside
  (`.unsaved-` and `.damaged-` files) is listed there too.
- **Undo keeps your own history when another program saves.** Claude or a second
  window saving the model no longer wipes your Undo: it takes back your last step and
  never the other program's work. If the other program changed or built on the same
  objects, Undo says so and keeps that step (press Undo again to skip it) instead of guessing. A plugin run is
  still one step.
- **A library of scale figures** for paths and streets (Library › Scale
  figures): a walker, a shopper with a tote bag, a jogger, a child, a dog
  walker, someone pushing a stroller, a wheelchair user and a cyclist, all flat
  and always turning to face you, plus a flat car and a flat SUV in profile for
  elevations and sections (those stay put; rotate them along the road). Search
  "people" to find them all. Scripts can `insert` them by name.
- **Plugins window** (File › Plugins ▸ Manage plugins…, or Shift+S): every
  plugin with its version and author, turn each on or off, remove yours
  (moved aside, so it can come back), and **Install from file…** for a .zip.
  Installs are checked: safe file names only, plugin files only, size limits.
- **Plugin forms.** `ctx.ask()` shows a form (lengths in the model's units,
  numbers, text, choices, checkboxes) and returns the answers; time spent
  filling it in doesn't count toward the one-minute limit.
- **SketchUp Ruby scripts.** Many SketchUp extensions that build things run
  in SketchCraft: install a `.rbz` or `.rb` from the Plugins window and its
  menu items appear in File › Plugins. Input boxes become forms, lengths are
  SketchUp's inches, and what a script builds is undoable. It runs in the same
  sandbox as other plugins; what isn't supported yet stops with a clear
  message. See docs/dev/sketchup-ruby.md.
- **Plugin tools.** A plugin can add an interactive tool: it gets snapped
  clicks and moves, draws previews, shows its hint, and keeps going until Esc.
- **Framing** (a built-in example): the **Stud wall** tool. Click where a wall
  starts and ends; plates and studs at your spacing (16" on center by default),
  every board its own group, walls chaining corner to corner.
- **Plugin buttons on the tool bar.** A plugin command with an icon gets a
  button, in its own group at the end of the bar; Cut list and Shelves have one.
- **More help in the app.** Learn has three new lessons (move a single
  corner, plugins, making it look your way), every lesson can play its demo
  first, eleven demos to watch (recorded from the app in the Classic look), and
  **Read the guide** opens the new user guide.
- **A user guide** in ten chapters (docs/guide), and developer docs
  (docs/dev: scripting and every command, MCP, building, architecture).
- **Shelves** (a built-in example plugin): a shelf unit from width, depth,
  height and shelf count, every board its own group, ready for Cut list.

## [0.3.5] - 2026-11-16

### Added
- **Scale figures.** New models start with a person standing at the origin,
  a flat figure that always turns to face you, for a sense of size (as in
  SketchUp). Pick who it is under **Document › Scale figure**: a person, the
  SketchCraft beaver with a clipboard, or nobody. On April 1, the beaver
  clocks in. Both are in the Library, renders turn them to the camera too,
  and any component can be set to **Always face the camera**.
- **Move a single corner.** With nothing selected, the Move tool grabs the
  corner under the cursor: its edges stretch as you go, and the faces around
  it fold to stay flat (as in SketchUp).
- **Windows portable ZIP.** `SketchCraft_<version>_x64_portable.zip`: unzip
  anywhere and open SketchCraft.exe, no installer. When a new version is out,
  the portable copy links to the new ZIP instead of installing.
- **Classic layout.** The tool bar, view bar and workspace strip dock edge to
  edge above the model, outside it, like SketchUp, and the Value box sits in
  the corner. Themes › Layout, Shift+S › **Classic layout**, or per workspace under
  Customize › Toolbar.
- **Plugins.** Small JavaScript plugins add commands (File › Plugins ▸, and
  Shift+S). They run sandboxed: no network, no files, only engine commands,
  and what they change can be undone. Drop a folder in
  ~/Documents/SketchCraft/Plugins; see [docs/PLUGINS.md](docs/PLUGINS.md).
- **Cut list** (the first plugin). Lists the parts to cut, grouped by size and
  material, and roughly how many 4×8 sheets and 8 ft boards to buy. Copy the
  table or save it as CSV.
- **Icons in themes.** A theme can bring its own tool icons: Themes › Icons ›
  **Load icons…** takes image files named after the tools (line.svg,
  push.png…; **File names** lists them). They're saved in the theme file, so
  an icon set can be shared like any theme.

### Changed
- **Classic is the new default.** New installs start in the Classic theme
  (neutral greys, a blue accent) with the tool bars docked above the model,
  at home for SketchUp users. SketchCraft's own Workshop look and floating
  tool bars, and every other theme, are in the Themes gallery; picking one
  brings its layout. Your current theme is kept.
- Every field (dropdowns, text and number boxes) shares one look from the
  theme: corners, border and height.
- **Skins are now Themes.** The **Themes** button (top right, with a palette)
  opens one window for the look: **Theme** (the gallery), **Layout** (Classic
  or Floating, with a picture of each) and **Icons** (built in, or load your
  own). Theme files are still .sketchskin, so shared ones keep working.

## [0.3.4] - 2026-11-15

### Changed
- **Menus fit the window.** The File menu is shorter: every export is under
  one **Export ▸** entry. All menus scroll when the window is short, and the
  right-click menu opens upward or leftward near an edge instead of running
  off the screen.
- **Fields on brand.** Every dropdown, text box and number box shares one look
  from the skin: its border and corners, a brand chevron instead of the
  browser's arrow, a violet focus ring, and violet checkboxes, sliders and
  color wells. Dark skins follow along.
- **The side panel fits.** Dropdowns in the right panel (Style's Preset, Faces
  and Background) stay inside it and line up, instead of running off the edge.

### Added
- **Axis gizmo.** The red, green and blue axes sit in the model's top-right
  corner and turn with the view. Drag it to orbit; click an axis to look along
  it (Z for the top view, the hollow −Y for the front), and click the axis
  facing you again to look from the other side.
- **See your version.** The bottom of the File menu shows which SketchCraft
  you're running, and **About SketchCraft** in Shift+S says it too.

## [0.3.3] - 2026-11-15

### Added
- **Automatic updates.** The desktop app checks for a new version a few
  seconds after it opens (and every six hours), and offers **Update and
  restart**. Updates are signed and verified before they install. **Help ›
  Check for updates…** (in the File menu and Shift+S) checks right away. Where a
  copy can't update itself, it links to the download page.

### Changed
- **The tool bar fits the window.** Tools that don't fit go behind a
  **+N more** button instead of running off the edges. Its list is searchable:
  type a tool's name (or shortcut) and press Enter. **+ Add or remove tools…**
  at the bottom customizes the bar.

## [0.3.2] - 2026-11-15

### Added
- **Center and Origin snapping.** Drawing, Move and Tape Measure now snap to the
  center of any arc or circle (a green ring, "Center") and to the model's origin
  (a yellow square, "Origin"), so you can move an object from a circle's center
  or put it back at the origin. Thanks to Common User on the ArtCraft Discord
  for the report.
- **Render settings.** The Render dialog's new **More settings** holds the
  output size (HD, 4K, square, portrait or custom), exposure, sunlight, sky
  light, sun color (cool to golden hour), light bounces, a plain background
  color instead of the sky, the ground on or off and its color, and denoise.
  They're remembered between renders. The `photoreal` command takes the same
  options (`sky_strength`, `background`, `ground`, `ground_color`, `warmth`).

### Fixed
- **Windows: DWG and IFC helpers are found.** SketchCraft now finds
  `dwg2dxf.exe` and the ODA File Converter in its versioned
  `Program Files\ODA` folder, and finds Python for IFC as `python.exe`, `py` or
  `%LOCALAPPDATA%\SketchCraft\ifc`. Opening an IFC file without IfcOpenShell
  shows Windows setup steps.
- The README banner and brand images are in the repository (an ignore rule had
  kept them out).

## [0.3.1] - 2026-11-15

### Fixed
- **macOS:** the whole app is signed (ad hoc, with the hardened runtime), so a
  downloaded copy no longer looks "damaged". macOS still asks the first time,
  since it isn't notarized yet: **System Settings › Privacy & Security › Open
  Anyway**.
- The release workflow signs and notarizes with a Developer ID once the Apple
  secrets are set (see [docs/SIGNING.md](docs/SIGNING.md)).

## [0.3.0] - 2026-11-15

Open the files architects send you, and draw like SketchUp.

### Added
- **Files**
  - AutoCAD DWG, open and save (with LibreDWG or the free ODA File Converter
    installed). DXF brings in text, dimensions, splines, ellipses and hatches.
  - IFC, so Revit and ArchiCAD models open (with IfcOpenShell installed). Each
    element becomes a group, typed and tagged by storey.
  - Point clouds: PLY, LAS, XYZ and PTS scans, in color. Drawing tools snap to
    scan points.
- **Floor plans to 3D:** right-click an imported floor plan and choose
  **Raise walls** to pull its walls up, with window openings (sill and head).
  Imports land on the drawing, not the sheet border around it.
- **Drawing and selecting**
  - Arc from its center, 3-Point Arc, Pie, Rotated Rectangle and Freehand.
  - Double-click a face for it and its edges; triple-click for everything
    connected.
  - Doors and windows cut their opening in the wall they're placed on, through
    thick walls.
  - The Axes tool. Tape Measure rescales the model.
  - Shift+S searches every command. The outliner has search. Hide Rest of
    Model, Paste in Place.
- **Scenes and looks**
  - Scenes remember their style, tags, hidden things and section cut. Named
    section planes.
  - Physically based finishes (Matte, Satin, Gloss, Metal, Glass, Glow) in
    renders, glTF and a new view style.
  - Dynamic Components: position, rotation and Component Options.
  - Bundled Bricolage Grotesque and IBM Plex Mono.

### Changed
- IFC export validated with IfcOpenShell.

### Security
- The dev bridge refuses other websites and DNS rebinding; a strict content
  security policy; importers fuzzed (no crashes in 20,000-file runs);
  dependencies updated (`cargo audit`: no vulnerabilities). See
  [docs/SECURITY.md](docs/SECURITY.md).

## [0.2.0] - 2026-11-13

The first build with installers (never published as a release).

### Added
- The engine: welded vertices, edges and faces with holes, push/pull, face
  splitting, groups, components, tags, guides, soft edges and saved views, all
  driven by JSON commands with undo and redo.
- The desktop app (Tauri and React): SketchUp-style tools and shortcuts, an
  outliner, entity info, a measurement box, dimensions and text, and templates.
- An MCP server (`sketchcraft mcp`) and CLI that edit the same live model as
  the app.
- An asset library (doors, windows, trees, cars, streetlights and more), OBJ
  import and export, and PNG renders with dimensions and labels.
- Installers for macOS, Windows and Linux, and the CLI for each.

[Unreleased]: https://github.com/jasonsutter87/sketchcraft/compare/v0.3.9...HEAD
[0.3.9]: https://github.com/jasonsutter87/sketchcraft/compare/v0.3.8...v0.3.9
[0.3.8]: https://github.com/jasonsutter87/sketchcraft/compare/v0.3.7...v0.3.8
[0.3.7]: https://github.com/jasonsutter87/sketchcraft/compare/v0.3.6...v0.3.7
[0.3.6]: https://github.com/jasonsutter87/sketchcraft/compare/v0.3.5...v0.3.6
[0.3.5]: https://github.com/jasonsutter87/sketchcraft/compare/v0.3.4...v0.3.5
[0.3.4]: https://github.com/jasonsutter87/sketchcraft/compare/v0.3.3...v0.3.4
[0.3.3]: https://github.com/jasonsutter87/sketchcraft/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/jasonsutter87/sketchcraft/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/jasonsutter87/sketchcraft/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/jasonsutter87/sketchcraft/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/jasonsutter87/sketchcraft/releases/tag/v0.2.0
