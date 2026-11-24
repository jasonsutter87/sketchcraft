<p align="center"><img src="brand/banner.png" alt="SketchCraft: Draw an outline. Pull it up." width="900"></p>

# SketchCraft

**Design your home, your furniture and your next project in 3D, for free.**
Draw an outline, pull it up into a shape, and keep going. It's quick to learn,
it works offline, and it opens your SketchUp files.

**Always free. Never freemium. Never paid.** No account, no subscription, no
watermark, nothing locked behind an upgrade. Mac, Windows and Linux.
[trysketchcraft.com](https://trysketchcraft.com/)

<p align="center"><img src="docs/demos/first-model.gif" alt="Drawing a rectangle, pulling it up into a box, typing exact sizes" width="860"></p>

## Download

**[Get the latest version](https://github.com/jasonsutter87/sketchcraft/releases/latest)**

| Your computer | Download |
| --- | --- |
| **Mac** (Apple silicon or Intel) | `SketchCraft_…_universal.dmg`, then drag SketchCraft to Applications |
| **Windows 10 or 11** | `SketchCraft_…_x64-setup.exe`, or the **portable ZIP** if you'd rather not install |
| **Linux** (x64 or arm64) | `.AppImage` (any distro), `.deb` or `.rpm`: `amd64` / `x86_64` for most PCs, `aarch64` / `arm64` for ARM machines like a Raspberry Pi 5 |

The first time you open it, your computer may ask whether you trust it,
because SketchCraft isn't signed by Apple or Microsoft yet:

- **Mac:** System Settings › Privacy & Security › **Open Anyway**.
- **Windows:** **More info › Run anyway**.

After that it **updates itself**: when a new version is out, it offers to
update when you open it.

## What will you make?

**A room, a remodel, a whole house.** Drag out a room like in The Sims, pull a
wall to make the room bigger, and drop in doors, windows, kitchens, bathrooms
and furniture from the Library. Turn a 2D floor plan into walls in one click.
Not sure where to start? Open **Library › Examples › Townhouse** and look around.

![A Craftsman house modeled in SketchCraft](docs/craftsman_street.png)

**Furniture and woodworking.** Draw a bookcase or a cabinet and SketchCraft
tells you what to cut and what to buy: a cut list with real lumber names
(2×4, 1×6) and board feet, then **Tools › Cut plan…** lays the parts onto
sheets and boards with cutting diagrams, prices and printable labels. Lay every
part out flat with one click, add dados, rabbets and tenons, and export a CNC
file for every part.

<p align="center"><img src="docs/demos/shelves-cut-list.gif" alt="The Shelves plugin builds a bookcase; Cut list lists its boards and what to buy" width="860"></p>

**Things to 3D print.** **Tools › Fix model** closes holes and cleans up a
messy model in one step, **Print check** tells you about thin walls, overhangs
and size before you print, and it saves STL and 3MF for your slicer.

**Drawings to build from.** Put your model on paper: plans, sections and
elevations on sheets at real scale, with dimensions that stay attached when the
model changes, line weights, hatches and tables. Print to PDF or send DXF.
Bring in CAD linework the other way: **Flatten to height** and **Make faces**
turn a DXF into surfaces you can pull up, and the **Curve** tool draws smooth
curves through the points you click.

**Pictures and walkthroughs.** Photoreal renders, sun studies, walkthrough
videos, and styles from sketchy to blueprint.

<p align="center"><img src="docs/demos/studio-render.gif" alt="Rendering a model in the studio" width="860"></p>

**Textures and game assets.** Paint straight onto your model, place pictures
exactly where you want them, unwrap UVs, or open a texture in PhotoCraft to
edit it in layers, and export to Unity, Unreal and Blender (FBX, glTF).

**Make it yours.** A SketchUp-like Classic look by default, or SketchCraft's own
themes, docked or floating tool bars, and your own shortcuts. In English,
Italiano, Français, Español, Deutsch, Português and 简体中文.

<p align="center"><img src="docs/demos/themes-layouts.gif" alt="Switching themes and layouts" width="860"></p>

## Coming from SketchUp?

You already know how. Same tools (Push/Pull, Follow Me, Offset, tape measure,
groups and components, tags), the same way of typing sizes as you go (`5,4`,
`8'6"`, `250cm`), and you can bring your SketchUp shortcuts with you. Your
`.skp` files open, from SketchUp 6 up to today's, and many of SketchUp's free
Ruby scripts run too.

## Opens your files

| | |
| --- | --- |
| **SketchUp** (`.skp`) | versions 6 to 8 and 2013 onward |
| **AutoCAD** (`.dwg`, `.dxf`) | DXF on its own; DWG with a free helper |
| **Revit, ArchiCAD** (`.ifc`) | with a free helper; SketchCraft shows how to set it up |
| **3D models** (`.fbx`, `.obj`) | |
| **3D scans** (`.ply`, `.las`, `.xyz`, `.pts`) | point clouds you can snap to |

It saves glTF, FBX, OBJ, STL and 3MF (3D printing), Collada, DXF, DWG, IFC, PDF, PNG and MP4.

## New to SketchCraft?

- **In the app:** click **Learn** (top right) for short lessons that watch you
  do each step, and demos to watch. Press **?** for every shortcut, and
  **Shift+S** to find any command by name.
- **The guide:** [Getting started](docs/guide/01-getting-started.md), then
  [Drawing](docs/guide/02-drawing.md), [Shaping](docs/guide/03-shaping.md),
  [Organizing](docs/guide/04-organizing.md), [Files](docs/guide/05-files.md),
  [Presenting](docs/guide/06-presenting.md),
  [Themes](docs/guide/07-themes.md), [Plugins](docs/guide/08-plugins.md),
  [Shortcuts](docs/guide/09-shortcuts.md) and
  [Troubleshooting](docs/guide/10-troubleshooting.md).

## Questions

**Is it really free?** Yes, for good. No account, no subscription, no watermark,
no paid tier. It's open source under the MIT license.

**Do I need the internet?** No. SketchCraft works offline; it only goes online
to check for updates and, if you ask, to fetch map imagery for a location.

**Does it run on my tablet or phone?** Not yet for everyone. Android tablets are
being tested now, and a touch-first version for phones comes later.

**Is it SketchUp?** No. It's an independent project that works the same way and
reads the same files. Not affiliated with Trimble or SketchUp.

**Something isn't working, or I have an idea.** See
[Troubleshooting](docs/guide/10-troubleshooting.md), or
[tell us about it](https://github.com/jasonsutter87/sketchcraft/issues). Many of
SketchCraft's features started as someone's request:
[what people asked for](FEATURE-REQUESTS.md) and [what's coming](ROADMAP.md).

---

# For developers

SketchCraft is a Rust engine with a React + three.js interface in a Tauri app.
Every action is a JSON command, so scripts, plugins and AI agents drive exactly
what you drive with the mouse.

## Scripting and the CLI

```json
{"cmd":"rect","from":[0,0,0],"to":[8,12,0],"as":"lot"}
{"cmd":"pushpull","face":"@lot","distance":6}
{"cmd":"insert","asset":"door","at":[3.5,0,0]}
{"cmd":"render","out":"house.png","view":"iso"}
```

```sh
sketchcraft run house.jsonl            # run a script
sketchcraft serve                      # JSON lines in and out on stdio
sketchcraft mcp --doc model.sketchcraft
sketchcraft bridge                     # the app's UI in a browser (localhost)
```

Every command: [docs/dev/scripting.md](docs/dev/scripting.md). The CLI is in each
release as `sketchcraft-cli-<platform>.zip`.

## AI agents (MCP)

`sketchcraft mcp` gives an agent `sketchcraft_run`, `sketchcraft_render` (it
gets the picture back, so it can see what it built) and `sketchcraft_info`.
Pointed at the app's live model, you and the agent build the same thing.
[docs/dev/mcp.md](docs/dev/mcp.md)

## Plugins

Plugins are small JavaScript files that add commands, ask questions with forms,
and build or measure geometry. They run sandboxed (no network, no files). A
subset of SketchUp's Ruby API runs too (`.rb` and `.rbz`).
[Writing a plugin](docs/PLUGINS.md), with the built-in
[Cut list](core/plugins/cutlist/main.js) and [Shelves](core/plugins/shelves/main.js)
as examples.

## Building from source

```sh
cargo build --release -p sketchcraft-core      # engine and CLI
cd app && npm ci && npm run tauri build         # the desktop app
```

Rust (stable) and Node.js 22. The browser dev loop, tests, demo GIF recording
and releasing: [docs/dev/building.md](docs/dev/building.md).

## How it's built

The model, commands, importers (including clean-room SketchUp readers),
exporters, a rasterizer and a path tracer live in `core/`; the interface in
`app/`. [docs/dev/architecture.md](docs/dev/architecture.md) ·
[renderer](docs/RENDERER.md) · [security](docs/SECURITY.md) ·
[SketchUp parity](docs/PARITY.md)

## Contributing

Bug reports and ideas: [issues](https://github.com/jasonsutter87/sketchcraft/issues). How to run the tests, and the rule that features land with them: [CONTRIBUTING.md](CONTRIBUTING.md).
Where SketchCraft is going, release by release: [ROADMAP.md](ROADMAP.md). What people asked for: [FEATURE-REQUESTS.md](FEATURE-REQUESTS.md).
What changed in each version: [CHANGELOG.md](CHANGELOG.md).

## License and trademarks

MIT, see [LICENSE](LICENSE). SketchCraft is an independent, clean-room project.
It is not affiliated with or endorsed by Trimble or SketchUp, or by ArtCraft.

[![M8ven Score](https://m8ven.ai/badge/mcp/jasonsutter87-sketchcraft-1xpwc9)](https://m8ven.ai/mcp/jasonsutter87-sketchcraft-1xpwc9?s=readme)
