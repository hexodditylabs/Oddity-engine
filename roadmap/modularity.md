# Modularity

*Decided 14 October 2026. How Oddity is split into pieces, so features can be
added, swapped or removed without touching the rest. The rules live in
[docs/architecture.md](../docs/architecture.md#modules-and-layers); this file
is the design behind them and the tasks that put them in place.*

**Contents:** [Goals](#goals) · [Layers](#layers) ·
[The shape of a feature](#the-shape-of-a-feature) ·
[The editor extension API](#the-editor-extension-api) ·
[Features in games](#features-in-games) ·
[Keeping it honest](#keeping-it-honest) · [Build plan](#build-plan)

---

## Goals

- **A feature is a set of crates.** Adding one means adding crates and one
  line of wiring. Removing one means deleting them. Nothing else changes.
- **Back ends are swappable.** Rapier today, Avian later, without changing a
  line of game code. The same goes for network transports.
- **Games ship only what they use.** A game without multiplayer doesn't compile
  or ship the networking code.
- **Built-in features and third-party plugins use the same door.** If physics
  can add itself to the editor through the extension API, so can anyone's
  plugin. This is how Godot builds its own editor features.
- **Every feature can be undone, automated and driven by an agent.** A
  feature changes things only through commands it registers, so undo, MCP,
  the CLI and scripts cover it with no extra work. The rules for commands are
  in [commands.md](commands.md).

## Layers

Every crate belongs to one layer. A crate may depend only on crates in its own
layer or a layer below it.

```
app         oddity-editor, oddity-hub                      wiring only
extension   oddity_physics_editor, oddity_steam_editor,     a feature's editor half
            oddity_net_editor, oddity_mcp, ...
tooling     oddity_core, oddity_widgets, oddity_icons,      what editors and the Hub are made of
            oddity_viewport, oddity_build, oddity_plugin_api,
            oddity_launch, oddity_hub
runtime     oddity (the facade games depend on)             what a shipped game may contain
            oddity_script, oddity_script_macros, oddity_level, oddity_project,
            oddity_physics, oddity_physics_rapier, oddity_steam,
            oddity_net, oddity_net_quic, oddity_net_steam, ...
──────────
Bevy =0.20.0 and third-party crates
```

Rules:

| Layer | May depend on | Why |
| --- | --- | --- |
| runtime | runtime | A shipped game must never contain editor code |
| tooling | runtime, tooling | The editor reads runtime types (levels, physics nodes) as data |
| extension | runtime, tooling, **not other extensions** | So any one extension can be removed on its own |
| app | anything | Apps only assemble plugins |

Three extra rules:

- **Only games depend on the `oddity` facade.** Workspace crates depend on the
  specific runtime crates they need. Otherwise the editor would link every back end.
- **Only the facade depends on a back end** (`oddity_physics_rapier`,
  `oddity_net_quic`, ...). Front crates like `oddity_physics` define the
  interface and never know which back end is underneath.
- **`oddity_core` stays headless:** it never depends on `bevy` (the full
  engine), only on `bevy_ecs`, `bevy_app` and `bevy_reflect`.

Dev-dependencies are exempt, so tests can use whatever they need.

"The editor never links or runs game code" still holds. The editor links
*engine* runtime crates such as `oddity_physics` so it knows their components.
It never links the *user's* game crate.

## The shape of a feature

Every feature follows the same pattern. Only the parts it needs are created.

| Crate | Layer | What it holds | Example |
| --- | --- | --- | --- |
| `oddity_<feature>` | runtime | Components, nodes, `Ctx` methods, signals, and a Bevy plugin that works in a game with no editor | `oddity_physics`: `RigidBody3D`, `Collision`, `ctx.raycast` |
| `oddity_<feature>_<backend>` | runtime | One implementation of the feature's back-end interface | `oddity_physics_rapier` |
| `oddity_<feature>_editor` | extension | The feature's **commands and queries**, node types, docks, Inspector fields, menu items, Project Settings pages, viewport drawing | `oddity_physics_editor`: `physics.set_shape`, physics nodes in Add Node, collision shapes in the viewport |

Each crate exposes one Bevy `Plugin`. Plugins are already Bevy's unit of
modularity, so we don't invent another one.

Guidelines:

- **One crate per feature, not per file.** Splitting further costs build time
  and boilerplate for nothing.
- **The front crate owns the interface.** `oddity_physics` defines what a back
  end must provide. `oddity_physics_rapier` implements it. Game code and the
  editor only see the front.
- **Features talk through signals and components, not each other's internals.**
  When one feature really needs another (the Steam transport needs Steam), it
  depends on the other's runtime crate, never on its editor crate.

## The editor extension API

`oddity_plugin_api` is the door into the editor. It's built in Phase 3 as an
internal, unstable API (`0.x`) so that every built-in feature uses it from day
one. Phase 6 makes it public: stable, versioned and documented, wrapped in the
`EditorPlugin` trait for third-party plugins.

It covers only the places where the editor has to *list* things. Anything a
plain Bevy plugin can already do, such as systems, observers and gizmos, an
extension does directly.

| Registry | What gets registered | First used by |
| --- | --- | --- |
| Commands | Edits, settings, actions and queries, each with its `CommandSpec` (the existing `CommandRegistry` in `oddity_core`) | Every feature; see [commands.md](commands.md) |
| Dock tabs | A tab in one of the docks (Scene, FileSystem, Inspector, Bottom), with its title, order and page | The existing docks (P2) |
| Menu items | An entry under a menu path, running a command by its ID | The existing menus (P2) |
| Node types | A name, a Godot icon, a category, and the components a new node starts with | Add Node, built-in nodes (P3) |
| Inspector fields | How to draw and edit one field type | Transform fields (P4), then `NodeRef`, `Prefab`, `Event` (scripting D3) |
| Project Settings pages | A page under Project Settings | Added with the Project Settings window; first pages are Input Map and Steam (S1) |

It's used like any Bevy app setup, through an extension trait on `App`. The
editor runs with a window or headless (CLI, MCP, tests), and an extension
always adds its commands, queries and node types, but its UI only when
there's a window:

```rust
impl Plugin for PhysicsEditorPlugin {
    fn build(&self, app: &mut App) {
        // Always: the CLI, MCP and tests can use physics too.
        app.register_node(NodeType::new::<RigidBody3D>("RigidBody3D", Icon::rigid_body_3d()))
            .register_node(NodeType::new::<Area3D>("Area3D", Icon::area_3d()))
            .register_command::<SetShape>(); // physics.set_shape

        // Only with a window.
        if app.editor_mode() == EditorMode::Window {
            app.add_menu_item("Physics/Fit Shape to Mesh", "physics.fit_shape")
                .add_systems(Update, draw_collision_shapes);
        }
    }
}
```

Three rules come with it:

- **Registries are read, never edited level data.** Creating a node from a
  `NodeType` runs the `node.create` command, so it can be undone, and an
  agent can do it over MCP.
- **The UI only runs commands.** A menu item is a command id. A dock button,
  a drag or an Inspector field runs a command too. Nothing in a dock changes
  the level or the project by itself.
- **Names in this API are a future public contract.** Choose them as carefully
  as the scripting API's.

## Features in games

The `oddity` facade turns each optional feature on with a Cargo feature:

```toml
# a game's Cargo.toml
[dependencies]
oddity = { version = "0.1", features = ["steam", "net-quic"] }
```

| Cargo feature | Turns on | Default |
| --- | --- | --- |
| `physics` | `oddity_physics` | on |
| `physics-rapier` | `physics` + `oddity_physics_rapier` | on |
| `steam` | `oddity_steam` | off |
| `net` | `oddity_net` | off |
| `net-quic` | `net` + `oddity_net_quic` | off |
| `net-steam` | `net` + `steam` + `oddity_net_steam` | off |

Phase 4 features add their own as they arrive (`navigation` for `landmass`,
and so on). Features built only on Bevy, such as animation and game UI, are
on by default.

`oddity::run` adds each enabled feature's plugin, and the prelude re-exports
its types only when it's on.

**The editor ships with every built-in extension**, because it's a prebuilt
program. It reads the project's enabled features and hides extensions the
project doesn't use: no Steam page or Network nodes in a single-player game.
Project Settings › Features turns them on and off by editing the game's
`Cargo.toml` (Phase 4).

## Keeping it honest

Rules that nothing checks wear away, fastest when code is written quickly.
So the layers are machine-checked.

Each crate declares its place in its own `Cargo.toml`:

```toml
[package.metadata.oddity]
layer = "runtime"        # runtime | tooling | extension | app | dev
role = "backend"         # optional: "facade" or "backend"
headless = true          # optional: must not depend on `bevy`
```

`cargo xtask layers` reads `cargo metadata` and fails if any rule above is
broken, or if a workspace crate doesn't declare a layer. CI runs it next to
fmt, clippy and tests.

## Build plan

Each task is one Claude Code session. P1 and P2 come first in Phase 3, before
the Inspector, Viewport and Nodes tracks, together with the command tasks K1
to K4 in [commands.md](commands.md#build-plan). Suggested order: P1, K1, K2,
P2, K3, K4, then P3 and P4. P3 and P4 replace the matching items in those
tracks. Scripting milestones A to C don't depend on any of these, so they can
go in parallel.

- [x] **P1. The layer check.** Add `[package.metadata.oddity]` to every
  existing crate (`oddity_core`: tooling, headless; `oddity_icons`,
  `oddity_widgets`: tooling; `oddity_project`: runtime; `oddity-editor`: app).
  Create `tools/xtask` (layer `dev`; depends only on `serde_json`, already a
  workspace dependency) with `cargo xtask layers`, add `tools/*` to the
  workspace members, add the `xtask` alias to `.cargo/config.toml`, and add a
  CI step.
  *Done when:* the check passes on the workspace, and tests prove it fails for
  each broken rule (a runtime crate depending on a tooling crate, one
  extension depending on another, a non-facade crate depending on a back end,
  a headless crate depending on `bevy`, a crate without a layer).
- [x] **P2. `oddity_plugin_api` v0: modes, commands, dock tabs and menu
  items.** Create the crate (layer tooling) with the `App` extension trait:
  `editor_mode()` (`Window` or `Headless`), `register_command` (onto
  `oddity_core`'s `CommandRegistry`), and the dock-tab and menu-item
  registries. Menu items take a command id only. The existing docks
  (`apps/editor/src/docks/mod.rs`) and menus then build from the registries
  instead of fixed lists, and every existing menu item runs a registered command.
  *Done when:* `ODDITY_SCREENSHOT` shows the editor looking exactly as
  before; a test plugin that registers one extra tab, one command and a menu
  item for it makes them appear; and the same plugin in a headless app adds
  the command without the tab.
- [x] **P3. Node types.** `register_node`, the `NodeType` registry, the
  `node.create` command (Edit, with an example) and the `node_types.list`
  query. The built-in nodes (Node3D, MeshInstance3D, Camera3D, the lights)
  register through it, and Add Node lists what's registered, with icons and
  categories.
  *Done when:* adding a node from the registry is one undo step, a node type
  registered by a test plugin shows up in Add Node and in `node_types.list`,
  and `node.create` passes the round-trip check (K4).
- [x] **P4. Inspector fields.** `add_inspector_field::<T>()`. The existing
  Transform x/y/z fields move to it. Fields edit through `component.set`, so
  scrubbing merges into one undo step (K3).
  *Done when:* the Inspector looks as before, a test plugin's field type is
  drawn with its registered field, and dragging a Transform value is one undo
  step.

Later, with the first optional feature (Steam, Phase 4):

- [ ] **P5. Project features.** `oddity_project` reads and writes the game's
  `oddity` Cargo features. Project Settings › Features turns them on and off
  through the `project.set_feature` command (a Setting, so it can be undone,
  and an agent can do it too).
  Extensions declare the feature they need and stay hidden without it.
  *Done when:* turning Steam on in Project Settings shows its settings page
  and nodes; turning it off hides them and removes the feature from the game's
  `Cargo.toml`.

Phase 6 then turns `oddity_plugin_api` into the public `EditorPlugin` API v1:
stable names, a version number, asset importers, docs and a sample plugin.
