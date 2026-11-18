# Scripting and the CLI

Everything SketchCraft does is a JSON command. The mouse, the CLI, plugins and
AI agents all send the same ones, so a script reproduces exactly what you'd do
by hand.

## A script

One command per line (`.jsonl`). Lines starting with `#` are comments. Units
are meters, Z is up. `"as": "name"` remembers a result's id; use `"@name"`
later.

```json
{"cmd":"rect","from":[0,0,0],"to":[8,12,0],"as":"lot"}
{"cmd":"pushpull","face":"@lot","distance":6}
{"cmd":"insert","asset":"door","at":[3.5,0,0]}
{"cmd":"dimension","from":[0,-1,0],"to":[8,-1,0],"offset":[0,0,-1]}
{"cmd":"render","out":"house.png","view":"iso"}
```

Every reply is JSON with `"ok": true` (and whatever the command returns, such
as `face`, `group` or `instance` ids) or `"ok": false` with an `error`.

## The CLI

Download `sketchcraft-cli-<platform>.zip` from the releases, or build it
(`cargo build --release -p sketchcraft-core`; the binary is
`target/release/sketchcraft`).

```sh
sketchcraft run examples/craftsman_house.jsonl     # run a script
sketchcraft serve                                  # JSON lines in, JSON lines out (stdio)
sketchcraft mcp --doc model.sketchcraft            # MCP server for AI agents
sketchcraft bridge                                 # the app's UI in a browser (localhost)
```

The `examples/` folder has full models built this way: a Craftsman house, a
townhouse, a row of houses, a retail center and the asset lineup.

## Commands

| Area | Commands |
| --- | --- |
| Draw | `rect`, `polygon`, `line`, `circle`, `arc`, `offset`, `text_3d` |
| Shape | `pushpull`, `follow_me`, `bevel`, `divide`, `slice`, `subd`, `subdivide`, `cage_edit` |
| Transform | `move` (faces, edges, groups, components or single points), `rotate`, `scale`, `duplicate`, `array`, `copy`, `flip` |
| Solids | `solid` (union, subtract, intersect, trim, split, outer shell), `intersect` |
| Organize | `begin_group` / `end_group`, `group`, `ungroup`, `set_parent`, `lock`, `edit` / `close`, `make_component`, `place`, `explode`, `make_unique`, `components`, `rename`, `connected`, `same_as` |
| Components | `component_cuts`, `component_face_me`, `set_attrs`, `options` |
| Library | `insert` (door, window, garage_door, tree, palm, car, streetlight, bench, hydrant, person, beaver; scale figures walker, shopper, jogger, child, dog_walker, stroller, wheelchair, cyclist, car_flat, suv_flat), `assets`, `wall`, `stairs`, `roof`, `column`, `library`, `save_to_library` |
| Paint | `paint` (a color or material), `material` (color, texture, roughness, metalness, glass, emission), `texture`, `opacity`, `finish` |
| Visibility | `hide`, `unhide`, `tag`, `tag_visible`, `tag_color`, `rename_tag`, `soften`, `weld`, `explode_curve` |
| Notes and views | `dimension`, `text`, `guide`, `clear_guides`, `save_view`, `show_view`, `delete_view`, `section`, `section_plane`, `section_off`, `set_axes` |
| Terrain and site | `terrain_from_contours`, `terrain_grid`, `smoove`, `stamp`, `drape`, `flip_edge`, `add_location`, `environment`, `sun` |
| Photos and scans | `match_photo`, `photo_solve`, `project_photo`, `import_cloud`, `cloud`, `cloud_data` |
| Inspect | `info`, `faces`, `pick`, `parts`, `volume`, `report`, `classify`, `units` |
| Pictures and video | `render`, `photoreal`, `animate`, `timeline`, `timeline_frames`, `export_sheets` (a PDF of the saved sheets), `blueprint` |
| Drawing sheets (saved in the model) | `sheets` (makes sheets from a page description), `sheet_add`, `sheet_edit`, `sheet_delete`, `viewport_add`, `viewport_edit`, `sheet_item_delete`, `sheets_info` |
| Files | `new` (optionally with a `figure`), `save`, `open`, `import_skp`, `import_dxf`, `import_dwg`, `import_ifc`, `import_fbx`, `import_obj`, `export_glb`, `export_obj`, `export_stl`, `export_dae`, `export_dxf`, `export_dwg`, `export_ifc`, `export_pdf`, `raise_walls` |
| Plugins | `plugins`, `plugin_enable`, `install_plugin` (`preview: true` describes a plugin without installing it; replacing an installed one needs `replace: true`), `remove_plugin` |

Scripts, the CLI and the app run every command. Plugin code and AI agents run
under a narrower allowlist (see [Security](../SECURITY.md)); every caller's numbers
(within +/-1e9), strings and paths are checked.
| History | `undo`, `redo` |
| Backups (the app only: not plugins or agents) | `list_backups`, `restore_backup` (`kind`: `backup`/`unsaved`/`damaged`, `name`, `mode`: `replace` or `copy`) |

The engine's own list (and each command's fields) lives in
[core/src/commands/command.rs](../../core/src/commands/command.rs): every command is one
variant of the `Command` enum, with a doc comment.
