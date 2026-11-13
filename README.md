# Riseline

**Draw an outline. Pull it up.**

Riseline is a push/pull 3D modeler for buildings and streets, written in Rust.
The engine is headless: every action is a JSON command, so the desktop app,
scripts, the CLI and AI agents (over MCP) all drive the same thing.

![Sunset row houses, built from commands](docs/sunset_iso.png)

## Try it

```sh
cargo run --release -- run examples/sunset_row_house.jsonl
open out/sunset_iso.png out/sunset_street.png
```

## Commands

One JSON object per line. Add `"as":"name"` to remember a result's id, then
use `"@name"` anywhere an id goes.

| Command | What it does |
|---|---|
| `rect {from, to}` | Axis-aligned rectangle (ground, or on a wall: it cuts a hole there) |
| `polygon {points}` | Any planar shape: footprint, gable, stair profile |
| `line {points}` | Lines; closing the loop makes a face |
| `pushpull {face, distance}` | Pull out (+) or push in (−) along the face normal |
| `pick {at, facing?}` | The face under a point |
| `faces {group?, facing?}` | List faces with normal, center, area |
| `move {ids, by}` | Move faces, edges, vertices or groups |
| `copy {group, by, name?}` | Duplicate a group (a whole house) |
| `begin_group {name}` / `end_group` | New geometry goes into a group |
| `group {ids, name?}` | Group existing faces |
| `paint {ids, color}` | RGB 0..1 on faces or whole groups |
| `reverse {ids}` / `delete {ids}` | Flip or remove |
| `undo` / `redo` | 200 steps |
| `render {out, width?, height?, view?, camera?}` | PNG, no GPU needed. Views: iso, front, back, left, right, top |
| `save {path}` / `open {path}` | `.riseline` (JSON) |
| `export_obj {path}` | OBJ + MTL, one object per group |
| `info` | Counts, bounds, groups, materials |

Units are meters; Z is up.

## The app

```sh
cd app && npm install && npm run tauri dev     # desktop (Tauri)
# or in a browser:
cargo run --release -- bridge & (cd app && npx vite)   # open http://127.0.0.1:5174
```

Figma-style layout: layers and templates on the left, properties on the
right, a floating toolbar (V select, R rectangle, P push/pull, B paint,
E erase, O orbit; hold Space to orbit). Type a number and press Enter for
exact sizes. The model lives in `~/Documents/Riseline/live.riseline`; the app
reloads it when Claude edits it over MCP, so you can build together.

## Claude (MCP)

`.mcp.json` registers `riseline mcp` for Claude Code in this repo. Claude gets
`riseline_run` (commands), `riseline_render` (sees a PNG) and `riseline_info`,
all on the same live model as the app. Build the binary first:
`cargo build --release`.

## Import a googledriver block

```sh
python3 tools/twin_import.py ../googledriver/twin/20th-ave out/20th_ave.jsonl
cargo run --release -- run out/20th_ave.jsonl --save ~/Documents/Riseline/live.riseline
```

## Run modes

- `riseline run script.jsonl [--open f] [--save f]`: run a script.
- `riseline serve [--open f]`: JSON commands on stdin, replies on stdout.
- `riseline mcp [--doc f]`: MCP server for AI agents.
- `riseline bridge [--port 5175]`: localhost HTTP bridge for the browser UI.

## Roadmap

1. ✅ Engine, commands, headless render, OBJ export
2. ✅ MCP server so Claude can model directly
3. ✅ Desktop app (Tauri + React, Figma/Canva-style panels), live-linked to the engine
4. ◐ Snapping: endpoints, midpoints, 5 cm grid done; axis inference and guide lines next
5. ✅ Face splitting (doors that touch the ground, bands across faces)
6. ✅ Import googledriver twin blocks (footprints, measured heights, sloped streets)
7. Photo match: project Street View onto faces
8. Asset library: doors, windows, trees, cars
9. Soft edges (hide lines between coplanar faces), line tool, move tool, gable roofs
