# Commands, undo and automation

*Decided 14 October 2026. Every feature, built-in or plugin, changes things
through commands. That one rule gives undo and redo everywhere, lets an AI
agent drive the editor over MCP, and makes the editor scriptable. This file
says what every command must provide, and the tasks that finish the command
layer. It goes with [modularity.md](modularity.md): a feature is a set of
crates **and** a set of commands.*

**Contents:** [What exists](#what-exists) · [The rules](#the-rules) ·
[Four kinds of command](#four-kinds-of-command) · [Undo](#undo) ·
[MCP](#mcp) · [Automation](#automation) ·
[Keeping it honest](#keeping-it-honest) · [Build plan](#build-plan)

---

## What exists

Built in Phase 1, in `oddity_core`:

- `EditorCommand`: `label`, `apply`, `undo`, `undoable`, `output`, `discard`.
- `History`: undo and redo stacks, a 500-step limit, the unsaved-changes flag.
- `Batch`: several commands as one undo step, all or nothing.
- `CommandRegistry`: commands by dotted id (`entity.spawn`,
  `component.set`, ...) with JSON arguments and a `CommandSpec` (description
  and arguments), built for the CLI and MCP.

Added in Phase 3 so far: command kinds, examples and invocations (K1),
each step's `Source` with the `CommandExecuted` message (K2), and merging
during a gesture, `batch` by name and history groups (K3), and the
round-trip check with an example on every edit command (K4), and MCP v0:
`oddity-editor mcp <project>`, built from the registry (K6).

That's the right foundation. What's missing is what a *modular* editor needs:
commands that features add themselves, reads as well as writes, knowing who
made a change, one history per open document, and checks that every undo
really undoes.

## The rules

1. **If a person can do it, a command can do it.** Every menu item, button,
   shortcut, drag and field edit runs a registered command. The UI never
   changes the level or the project directly. Menu items in
   `oddity_plugin_api` take a command id, nothing else, so this can't be skipped.
2. **Features register their own commands.** Ids are `<feature>.<verb>`
   (`physics.set_shape`, `steam.set_app_id`). Adding a feature adds its
   commands, and they show up in MCP, the CLI and the command palette with no
   other code.
3. **Commands work without a window.** An extension's plugin always adds its
   commands, queries and node types. It adds docks, fields and viewport drawing
   only when the editor has a window. So the CLI, MCP and tests see every feature.
4. **Every command can describe itself.** It reports the id and JSON arguments
   that would recreate it (`invocation`). Every history step can therefore be
   shown, replayed or turned into a script, however it was started.
5. **One intention, one undo step.** Dragging a slider for two seconds,
   deleting five nodes, or an agent's ten-step refactor is one step each.
6. **Every step records who made it:** you, the CLI, an MCP agent, a plugin
   or a script. You can always see, and undo, what an agent changed.

## Four kinds of command

Every `CommandSpec` says which kind it is. The kind decides where it goes in
history, and how MCP describes it to an agent.

| Kind | What it does | History | Examples |
| --- | --- | --- | --- |
| **Edit** | Changes an open level or prefab | That document's history | `entity.spawn`, `component.set`, `physics.set_shape` |
| **Setting** | Changes project settings (`oddity.toml`) | The project's history | `input.add_action`, `steam.set_app_id` |
| **Action** | Does something outside the documents | None, so it can't be undone; marked `destructive` if it overwrites or deletes | `level.save`, `build.run`, `play.start`, `export.run`, `file.move` |
| **Query** | Reads, never changes anything | None | `level.tree`, `component.get`, `node_types.list`, `commands.list` |

File operations in the FileSystem dock (rename, move, delete) are actions,
as in Unity and Godot. Delete moves files to the system's trash, so it can
still be recovered.

## Undo

**One history per open document.** Each level tab and prefab tab has its own
history and its own unsaved-changes flag, and the project has one more for
settings. Undo acts on the document you're looking at. Commands take an
optional `document` argument, which defaults to the active one, so an agent
can edit a level that isn't on screen. This has to land before level tabs and
Prefab mode (Phase 3), because one shared history would let Ctrl+Z in one tab
silently change another.

**Merging continuous edits.** `EditorCommand` gains `merge`: a new command
may fold into the previous step instead of adding one. Scrubbing a number or
dragging a gizmo sends many `component.set` commands on the same field; they
merge into one step. Merging stops when you release the mouse.

**Grouping.** Three ways to make several commands one step:
- `Batch` from code, as today.
- The `batch` command by name: `{"commands": [{"id": ..., "args": ...}, ...]}`.
  All or nothing.
- `history.group_begin` / `history.group_end`, for an agent working over
  several tool calls. Everything in between becomes one step, labelled by the
  agent ("Agent: build a wall of crates").

**Who did it.** Each step stores its source: `User`, `Cli`,
`Mcp { client }`, `Plugin { name }` or `Script { file }`. The History dock
shows a small icon for anything you didn't do yourself, and the label says
who ("Rename Goblin · Claude").

**Not in history:** selection (as today), queries, actions, and anything
changed while the game is playing. Live edits during Play are thrown away on
Stop, as in Unity.

**Every executed command is announced.** A `CommandExecuted` message (id,
args, source, document, result) goes out after every command. The History dock,
the unsaved-changes dots, MCP notifications and script recording all listen
to it instead of each hooking into the editor separately.

## MCP

`oddity_mcp` is an extension crate built on `rmcp` (3.5, which doesn't depend
on Bevy). It contains no list of commands: it builds itself from the registry.

| MCP | Comes from |
| --- | --- |
| **Tools** | Every Edit, Setting and Action command. Queries are tools too, marked read-only. The tool's description and schema are the command's `CommandSpec`. |
| **Tool hints** | Query → `readOnlyHint`. Edit and Setting → not destructive, since they can be undone. Action → `destructiveHint` when its spec says so. |
| **Resources** | The level tree, the selection, each document's history, node types, script types, project files, the Output log |
| **Notifications** | `CommandExecuted`, so an agent sees what you change while it works |

When a feature is added, its commands are tools straight away. A plugin's
commands are too.

Every MCP call runs with `Source::Mcp`, so agent changes are labelled in
History and undo like your own. `rmcp` runs on tokio, so tool calls cross
into the Bevy world through a channel and run in an exclusive system, in order
with the UI's own edits.

**When.** A first version (stdio, headless: `oddity-editor mcp <project>`)
moves up to Phase 3, right after the command tasks below. Claude Code can then
build and check levels through the same tools an end user's agent will use,
instead of guessing from screenshots. Attaching to an editor that's already
open (HTTP) stays in Phase 6; MCP tools for the running game come later.

## Automation

Everything below is built from the same registry. There's no separate
automation API.

- **Command palette** (Ctrl+Shift+P, as in Godot): search every Edit, Setting
  and Action by name and description, and see its shortcut.
- **Command scripts:** a `.json` list of `{id, args}` steps.
  `oddity-editor run <project> fix.json` runs one headless, as one undo group,
  and `--dry-run` reports what would change without saving.
- **Recording:** select steps in History, then *Copy as script*. Every step
  knows its own `invocation` (rule 4), so any change you made by hand can be
  replayed on another level.
- **CI and tests:** the same scripts and queries run in `cargo test` and in
  CI, through the headless editor.
- **Later:** WASM plugins and Rust plugins call the same commands; editor
  tools that run game code (Unity's `[ExecuteInEditMode]`) go through the live
  link.

## Keeping it honest

Undo bugs are silent: they show up weeks later as a level that won't come
back right. So, as with the layer check, a test checks every command.

Every Edit and Setting command's `CommandSpec` carries an `example`: arguments
that work on the standard test level. A workspace test,
`every_command_round_trips`, builds a headless editor with every extension's
headless plugin, and for each command:

1. snapshots the document (everything under its root, serialized through reflection),
2. runs the example, and snapshots again,
3. undoes, and checks the first snapshot is back exactly,
4. redoes, and checks the second snapshot is back exactly,
5. rebuilds the command from its `invocation` on a fresh copy, and checks it
   gives the same result.

Snapshots compare structure, names and values, not raw entity ids, which can
differ between two copies of a level. Deleted nodes (disabled, waiting for
undo) aren't part of a snapshot.

A command without an example fails the test. So does a query that changes
anything.

## Build plan

Each task is one Claude Code session. K1 to K4 come at the start of Phase 3,
alongside modularity P1 and P2, before features start adding commands. K5 must
land before level tabs and Prefab mode. K6 follows K2.

- [x] **K1. Kinds, examples and invocations.** `CommandSpec` gains `kind`
  (Edit, Setting, Action, Query), `destructive` and `example`.
  `EditorCommand` gains `invocation` (its id and args). Classify the existing
  commands, and add the first queries: `level.tree`, `component.get`,
  `component.list`, `selection.get`, `history.list`, `commands.list`.
  *Done when:* `commands.list` returns every spec with its kind, and running
  any query leaves the history untouched.
- [x] **K2. Sources and `CommandExecuted`.** Running a command takes a
  `Source`; history steps keep it; the History dock shows it; every command
  sends `CommandExecuted`.
  *Done when:* a test runs commands from two sources and `history.list`
  reports each one's source, and the unsaved-changes flag is driven by the
  message.
- [x] **K3. Merging and grouping.** `EditorCommand::merge`; `component.set`
  merges consecutive edits of the same field until the edit ends; the `batch`
  command by name; `history.group_begin` / `history.group_end`.
  *Done when:* scrubbing a value over 50 frames is one undo step, a
  three-command `batch` sent as JSON undoes in one step, and a `batch` whose
  last command fails leaves the level unchanged.
- [x] **K4. The round-trip check.** `oddity_core::testing::round_trip` and the
  `every_command_round_trips` workspace test described above, with examples
  for every existing Edit command.
  *Done when:* the test passes, and a deliberately broken `undo` in a test
  command makes it fail with the command's id.
- [ ] **K5. One history per document.** `History` moves from a single World
  resource to one per document root, plus one for the project. Undo and redo
  follow the focused tab; commands take an optional `document`.
  *Done when:* with two levels open, undo in one leaves the other unchanged,
  and each tab's unsaved dot is right.
- [x] **K6. MCP v0.** `oddity_mcp` (layer extension, `rmcp` 3.5), started
  with `oddity-editor mcp <project>`: stdio, headless. Tools, hints, resources
  and notifications generated from the registry as described above, with
  `Source::Mcp`.
  *Done when:* an MCP client test spawns a node, sets a field, reads it back
  with a query and undoes, all through tools; and a command registered by a
  test extension appears as a tool with no change to `oddity_mcp`.

Phase 6:

- [ ] **K7. Palette and scripts.** The command palette, `oddity-editor run`
  with `--dry-run`, and History's *Copy as script*.
  *Done when:* a script copied from History replays on a fresh copy of the
  level and gives the same result.
- [ ] **K8. MCP v1.** Attach to an open editor over HTTP: an agent's changes
  appear live, in the same history as yours.
  *Done when:* with the editor open, an MCP client renames a node, the
  Hierarchy shows it at once, and Ctrl+Z in the editor undoes it.
