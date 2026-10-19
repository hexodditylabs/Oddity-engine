# CLAUDE.md

Oddity Engine is a Godot-looking editor for Bevy, with Unity-style scripting in
Rust and a Unity-Hub-style launcher. This file is for Claude Code: read it at
the start of every session.

## Read first

- `README.md`: what exists and how to run it.
- `docs/architecture.md`: crates and their layers, the command layer, the
  level root.
- `docs/conventions.md`: house rules. Follow them exactly.
- `roadmap/README.md`: phases and checklists.
- `roadmap/scripting.md`: the scripting design and its task list. Required
  reading for any scripting, level-file, Play or physics work.
- `roadmap/modularity.md`: layers, the shape of a feature, the editor
  extension API. Required reading before adding a crate or a feature.
- `roadmap/commands.md`: command kinds, undo, MCP and automation. Required
  reading before adding or changing any command, or anything the UI can do.
- `roadmap/game-features.md`, `roadmap/steam.md`, `roadmap/multiplayer.md`:
  the plans for Phases 4 and 5. Read the relevant one before working there.

## How to work

- **One task at a time.** Tasks come from the checklists in `roadmap/`. Read
  the whole task and the sections it points to before writing code.
- **Tick the task's checkbox** in the same change that finishes it, and say
  which checks you ran.
- **The spec is the contract.** Names and behaviour in `roadmap/scripting.md`
  are what users will write against. If something there is wrong or
  impossible on Bevy 0.20, stop and explain why, with evidence from Bevy's
  source, and propose a change to the spec. Don't quietly do something else.
- **Ask before** changing pinned versions, adding a dependency the task doesn't
  name, changing a public contract (the level format, the `oddity` prelude,
  `oddity_launch`, `oddity_plugin_api`; before Phase 6 the plugin API is
  internal, so tasks that name a change to it may make it), adding a crate
  the roadmap doesn't list, or deleting files you didn't create.
- Prefer small, complete changes over large half-done ones.

## Commands

```
cargo editor                                  # run the editor (dev, dynamic linking)
cargo editor -- samples/shooter               # with a project open
(cd samples/shooter && cargo run)             # play the sample game (its own workspace)
cargo editor -- mcp samples/shooter           # headless MCP server on stdio (oddity_mcp)
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask layers                            # crate layer rules
```

fmt, clippy, tests and the layer check must all pass before a task is done
(CI checks the same).
Rust 1.97.1 or newer.

**Checking the UI.** You can't see the editor window.
`ODDITY_SCREENSHOT=out.png cargo editor` saves a screenshot once the UI settles
and quits; open the image and look at it. For anything visual, also ask the
user for a screenshot. The look copies Godot 4.7: measure Godot (sizes,
colours) rather than guessing.

## Bevy 0.20: facts you may get wrong

Bevy is pinned to exactly `=0.20.0` and stays there. Never upgrade it or
suggest 0.21. When an API you remember doesn't compile, read the real one in
`~/.cargo/registry/src/*/bevy_*-0.20.0/` instead of guessing.

- **Messages vs events.** Buffered events are *messages*: `#[derive(Message)]`,
  `MessageReader`, `MessageWriter`, `app.add_message::<T>()`. *Events* are for
  observers: `commands.trigger(..)`, and observer functions take `On<E>`
  (`on.event_target()` is the entity).
- **UI is written with BSN.** `bsn! { .. }`, `bsn_list! { .. }`,
  `commands.spawn_scene(..)`. Children: `Children [ a -- b ]`. Reuse a scene
  function with `@name(args)`. Observers: `on(handler)`. Non-literal values go
  in braces: `{expr}`. A bare variable can't be a component; wrap it in a tuple
  struct. An `Option` of a scene works as an optional child.
- **No `.bsn` files.** Bevy 0.20 says the file format "is not yet released".
  Our levels and prefabs are our own JSON format (`oddity_level`).
- **Feathers** (`bevy_feathers`): `FeathersButton` with `@caption` and
  `@variant`; buttons send `Activate`. `FeathersTextInput` has no placeholder;
  use `oddity_widgets::field::search_field`.
- **UI layout.** `Node` has a `border_radius` field;
  `BorderRadius::px(top_left, top_right, bottom_right, bottom_left)`,
  `BorderRadius::all(px(4))`; `px()` and `percent()` build `Val`s.
- **Picking.** `PointerClick` (has `count` and `button`), `PointerOver`,
  `PointerOut`, `PointerPress`, `PointerRelease`, `PointerDrag*`. The `Hovered`
  component (`bevy::picking::hover::Hovered`) updates itself.
- **Entity disabling.** Entities with `Disabled` drop out of queries; the
  editor uses it for undoable delete.
- **Reflection.** The `reflect_auto_register` feature registers every
  `#[derive(Reflect)]` type, including its type data, on Windows, Linux, macOS
  and Web.
- **Third-party crates.** Many Bevy crates still target Bevy 0.19. Check a
  crate's `bevy` dependency before adding it. As of 14 October 2026:
  - Support Bevy 0.20: `bevy_rapier3d` 0.37, `bevy_replicon` 0.45,
    `bevy_replicon_quinnet` 0.21, `bevy_quinnet` 0.22.
  - Don't depend on Bevy at all, so they're fine: `steamworks` 0.13,
    `landmass` 0.9.
  - Still need Bevy 0.19, so don't add them: `avian3d`, `lightyear`,
    `bevy_steamworks`, `bevy_hanabi`, `bevy_landmass`, `vleue_navigator`,
    `bevy_ecs_tilemap`, `bevy_replicon_renet`.
- **Hot-patching** exists as the `hotpatching` feature (Phase 4, not before).

## Architecture rules

- Every change to the level goes through an `oddity_core` command, so it can be
  undone. A system that edits level entities directly is a bug.
- **Commands** (rules in `roadmap/commands.md`):
  - Anything a person can do in the UI is a registered command with an id
    `<feature>.<verb>`, so MCP, the CLI and scripts can do it too. A menu item
    is a command id. A dock never changes the level or project by itself.
  - Each command has a kind (Edit, Setting, Action, Query), a `CommandSpec`
    with a one-line description (it becomes the MCP tool description), and
    an `invocation`. Edit and Setting commands also need an `example` that
    changes the standard test level, with entities as node paths
    (`"Enemies/Goblin"`); `every_command_round_trips` runs it through do,
    undo, redo and replay (`oddity_core::testing`). An extension adds the
    same test with its plugin in the world.
  - One intention is one undo step: merge continuous drags, batch multi-node
    changes. A drag or scrub wraps its commands in
    `History::begin_gesture` / `end_gesture` (ended on mouse release), and
    commands that can be scrubbed implement `EditorCommand::merge`. Several
    changes at once are a `Batch` (or `batch` by name); an agent's work over
    several calls is `history.group_begin` / `history.group_end`.
  - Register commands, queries and node types in every mode. Add docks,
    fields and drawing only when `editor_mode()` is `Window`.
  - A button or shortcut that changes the editor's own state (a tool, a
    panel, the play controls) runs a command too: a `UiAction` in
    `apps/editor/src/actions.rs` (Action kind, not an undo step), registered
    by the plugin that owns the state.
  - Changes made while the game is playing are never recorded in history.
  - Commands touch only live nodes of the open level: check every target
    with `level::require_in_level` (exists, not deleted, under `LevelRoot`).
    The level root can't be deleted, moved, or lose `LevelRoot`. "No
    parent" means the top of the level, never the top of the `World`.
  - A command that creates something before it runs (`SpawnEntity`) cleans
    it up in `discard(world, false)`; `execute` calls that when the first
    `apply` fails.
  - When changing `History`, keep the save point right as steps drop off
    either end, and test it with a small `History::with_limit`.
  - The editor's UI runs commands as the user (`execute`, `run_command`);
    the CLI, MCP, plugins and scripts use the `_as` forms with their
    `Source`. UI that shows edits refreshes on `CommandExecuted`, not on
    `History` change detection.
- Level entities live under the `LevelRoot` entity; everything else is editor.
- `oddity_core` stays headless (no rendering, no window) and fast to test.
- **The editor never links or runs game code.** Script values are data in the
  editor; only the game program (`oddity::run`) runs scripts. (It does link
  engine runtime crates like `oddity_physics`, to know their components.)
- **Layers.** Every crate declares `[package.metadata.oddity] layer` (runtime,
  tooling, extension, app, dev) and depends only on its own layer or lower.
  Runtime never depends on editor crates. Extensions never depend on each
  other. Only games depend on the `oddity` facade, and only the facade depends
  on a back end. A new crate needs its layer before anything else.
- **A feature is a set of crates:** `oddity_<feature>` (runtime),
  `oddity_<feature>_<backend>`, `oddity_<feature>_editor` (extension). Each
  exposes one Bevy `Plugin`. Optional features sit behind a Cargo feature on
  `oddity`.
- **Don't hard-code features into the editor.** Docks, menu items, node types,
  Inspector fields and Project Settings pages are registered through
  `oddity_plugin_api`, by the editor's own docks too.
- Colours come from `ActivePalette`. Sizes taken from Godot become named
  constants that say so.

## Pitfalls we've hit

- **Rebuilt widgets lose per-entity state.** The tree and item grid respawn
  their rows when the list changes, so anything Bevy tracks per entity
  (`PointerClick::count`, hover, focus) resets. Key it by the item's key, as
  `oddity_widgets::double_click` does. A drag ends if its entity is
  despawned, so the Inspector doesn't rebuild during a gesture
  (`History::in_gesture`); the dragged field updates its own text.
- **No per-frame rebuilds.** `Update` systems that build lists or touch the
  disk run only on change (`Changed<..>`, `resource_changed`, or compare
  before taking `ResMut`, which marks the resource changed on its own).
- **Walking folders:** don't follow symlinks or junctions without a cycle
  guard, and `warn!` on IO errors instead of flattening them away.
- **Paths:** make user paths absolute (`std::path::absolute`, not
  `canonicalize`, which adds `\\?\` on Windows) before searching upward.
  Paths stored in project files are relative, use `/`, and have no `..`.
- **The level shares the editor's `World`.** Anything in it that acts on
  its own (a camera renders to the window, audio plays) must stay inert
  while editing: Camera3D nodes are created with `Camera { is_active: false }`.
  A node's type is its `NodeKind` (the level file's `kind`), which also
  picks its icon.
- **`#[script]` needs `use oddity::prelude::*;` in its module.** A game
  depends only on `oddity`, and Bevy's derives then refer to `bevy_ecs` and
  `bevy_reflect` relative to the call site; the prelude provides both.
  In an engine crate's tests (which can't depend on the facade), write
  `use oddity_script as oddity;` instead; only `oddity_script` itself is
  special-cased by the macro.
- **Prove a bug before fixing it:** write the failing test first, and keep
  it as the regression test.

## Code style

The full rules are in `docs/conventions.md`. The ones that matter most:

- Comments explain *why*, never restate *what*. No commented-out code, no
  unused code kept for later, no TODO without the phase it lands in.
- Every module starts with a `//!` header; every public item has `///` docs.
- Test behaviour ("undo brings back the deleted children"), not internals.
- Typed errors (`thiserror`) in libraries.

## Map

```
apps/editor/       the editor: chrome/ (title bar), docks/, layout, shortcuts
crates/
  oddity/          the facade games depend on: prelude, re-exports
  oddity_script/   scripts: the Script trait, Ctx, NodeRef, ReflectScript
  oddity_script_macros/ #[script]
  oddity_level/    level and prefab files (JSON): save, spawn, instances
  oddity_core/     commands, undo/redo, selection, level root (headless)
  oddity_widgets/  Godot theme, fonts, docks, tree, fields, buttons
  oddity_icons/    Godot and @icons SVGs, rendered at any size
  oddity_plugin_api/ editor mode, commands, dock tabs, menu items
  oddity_project/  oddity.toml projects, asset IDs, new games (template)
  oddity_build/    builds from the editor: watching, cargo messages, describe
tools/xtask/       `cargo xtask layers`: the crate layer check
samples/shooter/   the test game from roadmap/scripting.md (grows with the tasks)
docs/              architecture and conventions
roadmap/           the roadmap, the scripting and modularity specs, diagrams
third_party/       Godot and @icons artwork with their licences
```

The user works on Windows. Paths in examples should work there too.
