# AI agents over MCP

`sketchcraft mcp` is a [Model Context Protocol](https://modelcontextprotocol.io)
server with three tools:

| Tool | Does |
| --- | --- |
| `sketchcraft_run` | runs commands (one, or a list) and returns the replies |
| `sketchcraft_render` | renders the model and returns the image, so the agent can look at what it built |
| `sketchcraft_info` | what's in the model |

## Working alongside the app

Give `--doc` the desktop app's live file and you and the agent edit the same
model: what it builds appears in the app within a second, and what you do is
there for it.

```sh
sketchcraft mcp --doc ~/Documents/SketchCraft/live.sketchcraft
```

The live file is `~/Documents/SketchCraft/live.sketchcraft`, or the path in the
`SKETCHCRAFT_DOC` environment variable if you set one. The app and the agent
each pick up the other's latest save before they change anything, and save the
whole model after a change, so edits don't overwrite each other (if you both
change the same thing at the same moment, the later one wins). Backups of the
file are kept in `Backups/` beside it; an agent's own destructive steps (`new`,
`open`, big deletes) are backed up in a small separate quota (10 files, paced to one a
minute) so they can't push out yours. Each client has its own Undo: yours takes back
your last step and never the agent's work (and the other way round).

## What an agent may do

An agent isn't you, and a model can carry words written by someone else (names
in an imported file can read like instructions). So an agent gets less than the
person at the keyboard:

- modeling, inspection and rendering, and file import/export, but only **inside
  a workspace folder**: the folder of the `--doc` model file, or `--workspace DIR`
  to choose another (`--any-path` lifts the limit). There is no fallback to the
  current folder: with neither flag the server won't start, and a home folder or
  drive root is refused. Relative paths start in the workspace; hidden files,
  `Backups/` and `Plugins/` are off limits;
- it writes files with the right extension for the command (`.obj` for
  `export_obj`, `.png` for `save_image`, and so on), and never replaces a file it
  didn't make itself in that session (and that is still exactly as it left it): it writes new file names, never one that looks like a `.unsaved-` or `.damaged-` file. `save_image`
  only writes images it can decode. The library an agent uses is `Library/` in
  its workspace; `save_to_library` and `library_add` take plain `collection`/`name`
  values (no folders, dots or `Backups`/`Plugins`; a collection is ASCII letters, digits, space, `-` and `_`)
  and add new files only, and an
  OBJ or DAE export refuses to write beside an existing `.mtl` or `_textures`
  folder it didn't make;
- never: installing, enabling or removing plugins, `save_text` (writing any text
  to any file), `reveal`, `skins`, `video_frame`/`video_finish`;
- numbers outside +/-1e9, over-long strings and URL or pipe paths are refused
  for everyone;
- names and text read from the model are marked as data in the tool output, and the model's own words in a reply
  (group names, tags, labels) lose control, direction and invisible characters;
- every command it runs, read-only ones too, has a time limit, and so does a whole `sketchcraft_run` call (below).

### Time limits for agents and plugins

One engine-wide limit holds for everything an agent or a plugin asks for (`core/src/deadline.rs`). The person's own
commands from the app have none.

- **Where it is set.** `Engine::exec_value_inner` (`commands.rs`) knows the caller: for anything but the app's own user
  it runs the command inside `deadline::command_scope`, 10 s for most commands and 25 s for the two that are slow by
  nature (`photoreal`, `animate`). `mcp.rs` wraps a whole `sketchcraft_run` call in `deadline::call_scope` (30 s for
  all its commands together), and the picture tools (`sketchcraft_render`) run under the same command limit. The app's
  AI box holds its answer to the call limit too. The limit is per thread, so the loops of the engine reach it without
  being handed anything.
- **What checks it.** `Model::check_deadline` and `over_deadline` read it (so every older `check_deadline` call in a
  loop sees it, with the command's own, shorter, budgets still applying where they protect memory: curves 8 s,
  solids 10 s, copies, text). Loops that cannot return an error call `deadline::poll()`: a counter that reads the clock
  every 64th call and, past the limit, unwinds out of the command. `Model::new_id` polls, so anything that creates
  geometry is covered; the rest are the scans of drawing and splitting lines, the loop search, intersect, sweeps,
  booleans (their own thread honours it), importers, raster and render loops, unwrap and packing, texture painting,
  sheet drawing (its worker threads take the limit with them), terrain and walls. A new loop that can run long should
  call `poll()` (or `check_deadline()` where it returns a `Result`).
- **What happens when it expires.** The engine catches the unwind, puts the model back exactly as it was before the
  command (the same restore that a failed command uses) and replies with an error that says it ran out of time and that
  nothing changed (`timed_out: true`). If the whole call's limit was the one that ran out, the call stops and
  everything it did is undone (`Live::rollback_call`), with a line saying so; work from earlier calls stays.
- **What the person gets.** Nothing changes: no clock, no refusal, no rollback.
- Tests set the limits shorter with `deadline::set_limits_for_tests` rather than waiting for the real ones.

## Claude Code

This repository's `.mcp.json` points Claude Code at a local build under
`./target/release/` in the repository (start Claude Code from the repository's top folder; it runs that binary
when you trust the project, so only trust a clone whose binary you built). In another
project, add:

```json
{
  "mcpServers": {
    "sketchcraft": {
      "command": "/path/to/sketchcraft",
      "args": ["mcp", "--doc", "/Users/you/Documents/SketchCraft/live.sketchcraft"]
    }
  }
}
```

The commands an agent can send are the same as for scripts: see
[Scripting](scripting.md).
