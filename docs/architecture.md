# Architecture

How Oddity Engine is put together, and why. Read this before adding a crate
or a dock.

## The big picture

```
            Hub (Phase 5)
              │ launches the editor version a project pins
              ▼
┌──────────────────────── oddity-editor ────────────────────────┐
│  chrome/ (title bar, top bar)      docks/ (Hierarchy, ...)    │
│                │                          │                   │
│                └──────────┬───────────────┘                   │
│                           ▼                                   │
│           oddity_core: commands · undo/redo · selection       │◄── CLI, MCP, plugins
│                           ▼                                   │
│                Bevy World (level under LevelRoot)             │
└───────────────────────────────────────────────────────────────┘
```

**Every change to a level is a command.** The UI, keyboard shortcuts, the
CLI, MCP and plugins never modify level entities directly; they run an
`EditorCommand` through `world.execute(..)` or `world.run_command("id", json)`
(the editor's own UI), or their `_as` forms, which say who is asking: the
CLI, an MCP agent, a plugin or a script (`Source`). That single rule is what gives undo/redo everywhere, lets an AI agent drive
the editor, and keeps "unsaved changes" honest.

Reading is free: any system may query the level. Every command, whoever ran
it, is announced afterwards with a `CommandExecuted` message (id, arguments,
source, document, result). What shows the history or unsaved changes (the
History dock, the level tab's dot, the status line) and the Inspector
refresh on that message, so an MCP edit or an undo shows up the same way as
your own click. Queries change nothing and aren't announced.

The command rules every feature follows (the full design is in
[roadmap/commands.md](../roadmap/commands.md)):

- **If a person can do it, a command can do it.** Menu items are command ids;
  buttons, drags and fields run commands. Features register their own
  (`<feature>.<verb>`), and they appear in MCP, the CLI and the command
  palette with no other code.
- **Four kinds:** *Edit* (a level or prefab, undoable in that document's
  history), *Setting* (project settings, undoable in the project's history),
  *Action* (save, build, play, export, file moves: not undoable) and *Query*
  (reads only).
- **Commands work headless.** Extensions add their commands, queries and node
  types in every mode, and their UI only when there's a window.
- **One intention, one undo step**, and every step records its source (you,
  the CLI, an MCP agent, a plugin, a script).
- **Every Edit and Setting command has an example**, and one test runs do,
  undo, redo and replay on all of them.

## Crates

| Crate | Layer | What it owns | Depends on |
| --- | --- | --- | --- |
| `oddity_core` | tooling, headless | Commands, undo/redo history, selection, the level root, the command registry (JSON, for CLI/MCP), and scripts as data in the editor (`scripts`: `Scripts`, `ScriptTypes`, the `script.*` commands). No rendering. | `bevy_ecs`, `bevy_app`, `bevy_reflect` |
| `oddity_project` | runtime | `oddity.toml`: create, load, save, find, version pinning. Games read it too (the input map). Asset IDs: `.import` files and the `AssetIndex`. New games: `create_game` (the game crate, a main level, `.oddity/runner`, the shared target folder). | `serde`, `toml`, `serde_json`, `uuid` |
| `oddity_icons` | tooling | Godot, @icons, brand and UI icons as embedded SVG, rendered at any scale; `UiIcon`. | `bevy`, `resvg` |
| `oddity_widgets` | tooling | The theme (Godot's palette rules + Oddity accent), fonts, and editor widgets: docks, splits, tree view, search fields, tool buttons, hover feedback. Reused by the Hub and plugins. | `bevy` (+ Feathers), `oddity_icons` |
| `oddity` | runtime, facade | What games depend on: the prelude (Bevy's, plus scripts) and re-exports; `run`, the game program (`--oddity-play`, `--oddity-describe`, else `[editor] main_level`); `describe`, the type description; `test::Game`, the headless harness for testing game code (`Game::load` reads a level file); `input_map`, which turns `oddity.toml`'s `[input]` into the `InputMap`. | `bevy`, `oddity_script`, `oddity_level`, `oddity_project` |
| `oddity_script` | runtime | The `Script` trait and its callbacks, `Ctx`, `NodeRef`, `ReflectScript` type data, the Inspector attributes `#[script]` records, and the runner (`ScriptPlugin`): Hierarchy order, take-out/put-back, `_onready` timing, destroy callbacks, panics switched off. Events, signals, timers, input (`ctx.input()`, `InputMap`, bindings by Bevy's key names), and pause, time scale and `ProcessMode`. | `bevy`, `oddity_script_macros` |
| `oddity_script_macros` | runtime | `#[script]`: a Component with Reflect, `Default` from field defaults, saved fields marked `Export`. | `syn`, `quote` |
| `oddity_level` | runtime | Level and prefab files (JSON): `save_level`/`save_prefab` from a World, `spawn_level` into one, through reflection; `NodeRef`s as file ids that stay stable (`NodeId`); what can't be read kept as written (`UnknownData`). Prefab instances with overrides (`PrefabInstance`), the `PrefabLibrary`, and `LevelPlugin`, which gives `ctx.instantiate` its spawner. `MeshShape`: simple shapes saved inline until mesh assets. | `bevy`, `oddity_script`, `oddity_project`, `serde_json`, `uuid` |
| `oddity_mcp` | extension | The MCP server: every registered command and query as a tool, the level tree, selection, history and node types as resources, executed commands as notifications. Built from the registry, with no list of its own. `oddity-editor mcp <project>` serves it headless on stdio. | `bevy_app`, `bevy_ecs`, `oddity_core`, `rmcp`, `tokio` |
| `oddity_plugin_api` | tooling | The door into the editor: `editor_mode()` (window or headless), `register_command`, node types (`register_node`, `node.create`, `node_types.list`), Inspector fields (`add_inspector_field`), and the dock-tab and menu-item registries the editor builds itself from. Internal (0.x) until Phase 6. | `bevy`, `oddity_core`, `oddity_icons` |
| `oddity_build` | tooling, headless | The editor's side of game builds: `Watcher` (the code under `src/`), `Build` (cargo with JSON messages, in the background, then `--oddity-describe` into `.oddity/types.json`), `Diagnostic`s with file, line and column. | `oddity_project`, `serde`, `serde_json` |
| `oddity-editor` (app) | app | Puts it together: window chrome, docks, layout, shortcuts. | all of the above |

The crates still to come, and the phase each arrives in, are listed in
[roadmap/README.md](../roadmap/README.md#repository-layout).

Rules of thumb:

- **Logic goes in a crate, wiring goes in the app.** If the Hub, the CLI or a
  plugin could need it, it doesn't belong in `apps/editor`.
- **`oddity_core` stays headless.** It must never depend on `bevy` (the full
  engine), rendering or UI, so it builds in seconds and runs in CLI/MCP/tests.
- **Bevy is pinned at `=0.20.0`.** It is our seed, not a moving dependency.

## Modules and layers

Oddity is built so a feature can be added, swapped or removed without touching
the rest. The design and its tasks are in
[roadmap/modularity.md](../roadmap/modularity.md); these are the rules.

**Every crate is in one layer, and depends only on its own layer or lower:**

```
app         oddity-editor, oddity-hub            wiring only
extension   oddity_<feature>_editor, oddity_mcp  a feature's editor half
tooling     oddity_core, oddity_widgets, ...     what editors and the Hub are made of
runtime     oddity, oddity_<feature>, ...        what a shipped game may contain
```

- **Runtime never depends on tooling, extensions or apps.** A shipped game
  contains no editor code.
- **Extensions don't depend on each other**, so each can be removed on its own.
- **Only games depend on the `oddity` facade**, and **only the facade depends on
  a back end** (`oddity_physics_rapier`, `oddity_net_quic`, ...). Front crates
  define the interface; game code and the editor never see which back end runs.
- Dev-dependencies are exempt.

Each crate declares its layer in `Cargo.toml`
(`[package.metadata.oddity] layer = "..."`), and `cargo xtask layers` checks
every rule in CI.

**A feature is a set of crates with the same shape:** `oddity_<feature>`
(runtime: components, nodes, `Ctx` methods, a Bevy plugin that works without
the editor), optional `oddity_<feature>_<backend>` crates, and an optional
`oddity_<feature>_editor` (extension: its nodes, docks, fields and menus).
Each crate exposes one Bevy `Plugin`.

**Built-in features enter the editor the way plugins will.**
`oddity_plugin_api` holds the registries the editor builds itself from: dock
tabs, menu items, node types, Inspector fields and Project Settings pages. The
editor's own docks use them too. Don't hard-code a list of features into
`apps/editor`; register them. Anything a plain Bevy plugin can do on its own
(systems, observers, gizmos), an extension does directly.

**Games choose features with Cargo features on `oddity`** (`physics-rapier`
by default, then `steam`, `net-quic` and so on). The prebuilt editor carries
every built-in extension and hides the ones a project hasn't turned on.

## The level, and what isn't the level

Editor UI and the edited level live in the same Bevy `World`. Everything
under the entity marked `LevelRoot` is the level: the Hierarchy shows it,
saving writes it, Play runs it. UI entities are never under it, so they can't
be selected, saved or deleted by accident. `oddity_core::is_in_level` checks
membership. Prefabs opened in isolation (Phase 3) get their own root the same
way.

Oddity uses Unity's model: **levels** (`.level`) are worlds you play,
**prefabs** (`.prefab`) are reusable objects edited in isolation, and
**instances** are prefabs placed in a level, with overrides you can Apply or
Revert. Both are JSON files, written and read by `oddity_level` (see `roadmap/scripting.md`).

## How deleting works

`DespawnEntity` doesn't despawn: it inserts Bevy's `Disabled` component, which
hides the entities from every query by default. Undo removes it, so the very
same entities (same ids) come back, and anything else referring to them stays
valid. The entities are despawned for real when the step leaves the history
(the redo stack is cleared or the undo limit is reached), via
`EditorCommand::discard`.

## The editor app

```
apps/editor/src/
├── main.rs        assembles plugins; nothing else
├── layout.rs      where the docks sit and how they resize
├── level.rs       the level being edited (demo level until .level files load)
├── project.rs     project from the command line
├── shortcuts.rs   editor-wide keys
├── screenshot.rs  ODDITY_SCREENSHOT=file.png support
├── chrome/        title bar, top bar (menus, tabs, play), window icon
└── docks/         one module per dock; mod.rs builds the docks from their tabs
```

Adding a dock page: write `docks/<name>.rs` with a `<name>_page()` scene and
a plugin that registers it, in window mode only:
`app.add_dock_tab(DockTab::new(DockSlot::Bottom, "Name", name_page).order(..))`.
Built-in tabs are 100 apart (`FIRST_TAB`, `SECOND_TAB`), so a tab can go
between them. A menu item is `app.add_menu_item("Menu/Item", "command.id")`.
A feature's pages and menu items live in its own `_editor` crate and
register the same way.

## UI building blocks

- Layouts are written with Bevy's `bsn!` scene macro.
- Feathers (`bevy_feathers`) supplies standard controls; our theme recolours
  them and our fonts replace theirs (`oddity_widgets::fonts`).
- `oddity_widgets` adds what an editor needs on top: `dock` (side docks and
  Godot's bottom panel), `split`, `tree` (Godot's scene tree, relationship
  lines and row buttons included), `field::search_field`,
  `button::{icon_button, tool_button, Toggled}`, `hover::HoverFill`.
- Colours come from `ActivePalette` (derived like Godot's editor theme), never
  hard-coded hex values in the editor. The palette is checked against colours
  measured from a Godot 4.7 screenshot (`theme.rs`,
  `matches_godot_4_7_screenshot`), so it can't drift from Godot unnoticed.

## Matching Godot's look

Sizes come from Godot 4.7 at 100% scale, measured from screenshots: 40 px
dock tab bars holding 32 px tabs, 3 px gaps between panels, 4 px corner
radius, 22 px tree rows indented by 12 px, 28 px toolbar buttons and fields,
14 px text. When something looks off next to Godot, measure Godot first and
change the constant; don't eyeball it.
