# Oddity Engine — Godot-style Editor for Bevy: Roadmap

Oct 12, 2026 · @Bande5o

## Vision and principles

Oddity Engine, by Hex0ddity Labs, is a Godot-style editor for Bevy 0.20, written in Rust, with a Unity-Hub-style launcher on top. It copies Godot's look and icons, but underneath it uses Bevy's ECS, not Godot's nodes.

- **Godot look, Bevy model.** Four-dock layout, dark theme and Godot's MIT-licensed icons, with entities and components underneath.
- **One command layer.** Every action is a command. Menus, shortcuts, the CLI, MCP and plugins all call the same commands, which gives undo/redo everywhere.
- **Native Bevy UI.** Our own widget library on top of `bevy_ui_widgets`, modelled on `bevy_feathers`.
- **Hub for users, never required for development.** The Hub is the normal way in. Debug builds, headless commands and `--no-hub` skip it.
- **Pinned versions.** Each project pins its editor and Bevy version, and upgrades are deliberate.
- **Small steps.** Every phase ends in something that runs.

## Prerequisites

Install these before Phase 0. Everything else gets added when a phase needs it.

| Need | Why |
| --- | --- |
| Rust stable via rustup, kept current | Bevy tracks recent Rust releases |
| Editor with rust-analyzer (VS Code, RustRover or Zed) | Completion and inline errors |
| Fast linker (lld, or mold on Linux) plus Bevy's `dynamic_linking` feature for dev builds | Bevy's default build times are slow |
| Godot installed | Side-by-side visual reference |
| Godot source checkout | Icons in `editor/icons/` (SVG); theme colours and sizes under `editor/themes/` |
| `resvg` crate | Bevy doesn't render SVG, so icons are embedded as SVG and rendered at the exact size and screen scale at runtime |
| Bevy 0.20 release notes and migration guide | AI models may know older APIs; hand these over at the start of a session |

Knowledge needed: comfortable Rust (ownership, traits, generics), Bevy ECS basics (components, systems, queries, observers), and CSS-style flexbox for Bevy UI layout.

## Architecture

Two binaries share one workspace, and every way of changing a project goes through the command layer in `oddity_core`.

&#91;embedded content: architecture · Hub, editor, command layer\]

The Hub reads a project's `oddity.toml`, then starts the matching editor through the `oddity_launch` handoff. Inside the editor, the UI, CLI, MCP server and plugins never touch the World directly; they run commands, so undo/redo, scripting and AI control are one feature. Headless mode is the same editor with `MinimalPlugins` and `ScheduleRunnerPlugin` and no window, used by the CLI, MCP and tests.

## Repository structure

One Cargo workspace holds both apps and every crate, so the Hub and editor share the widgets, project format and launch contract. Create crates as phases need them; Phase 0 starts with `oddity_widgets`, `oddity_icons` and `apps/editor`.

```
Oddity-engine/
├── Cargo.toml                 # workspace root, pins bevy = "0.20"
├── .cargo/config.toml         # fast-compile settings, `cargo editor` alias
├── .github/workflows/ci.yml   # fmt, clippy, tests; Windows build
├── branding/                  # cat, wordmark, Hex0ddity mark (SVG), brand board, colours
├── crates/
│   ├── oddity_icons/          # Godot + @icons + brand SVGs, rendered at any scale   [Phase 0 ✓]
│   ├── oddity_widgets/        # own feathers: Godot palette, brand, fonts             [Phase 0 ✓]
│   ├── oddity_core/           # command registry, undo/redo, selection, headless mode
│   ├── oddity_project/        # oddity.toml format, templates, recent projects
│   ├── oddity_launch/         # Hub ↔ editor handoff contract (keep stable)
│   ├── oddity_scene/          # scene save/load on top of BSN
│   ├── oddity_inspector/      # reflection-driven property editor
│   ├── oddity_viewport/       # editor camera, grid, gizmos, picking
│   ├── oddity_docks/          # scene tree, filesystem, inspector docks
│   ├── oddity_plugin_api/     # EditorPlugin trait + extension points (keep stable)
│   ├── oddity_mcp/            # MCP server: commands → tools, scene → resources
│   └── oddity_hub/            # installs, version manifest, downloads, project registry
├── apps/
│   ├── editor/                # oddity-editor binary                                [Phase 0 ✓]
│   ├── hub/                   # oddity-hub binary: UI + CLI
│   └── sandbox/               # test game to edit
├── templates/                 # 2d/, 3d/ project templates
├── third_party/
│   ├── godot/                 # Godot 4.7.2 editor icons (1,024 SVGs) + MIT licence
│   └── at-icons/              # @icons by Voxybuns (642 SVGs) + MIT licence
└── THIRD_PARTY_NOTICES.md     # icon and font licences
```

Two crates are public contracts: `oddity_launch` (older editor versions must still launch) and `oddity_plugin_api` (third-party plugins depend on it). Change them rarely and version them.

## Widget library

We build our own `bevy_feathers` equivalent: a Godot-themed layer on Bevy's unstyled `bevy_ui_widgets`. Feathers' code is the starting point; its docs invite copying it into your own project.

| Widget | Source | Used by |
| --- | --- | --- |
| Button, checkbox, radio, slider, toggle, colour picker | Feathers, themed (Phase 0 ✓) | Everywhere |
| Text input, number input | Feathers in Bevy 0.20, themed | Inspector |
| Menu, select, list view, dialog, scrollbar | Feathers in Bevy 0.20, themed | Menus, inspector, Hub |
| Pane, pane header, subpane, group | Feathers in Bevy 0.20, themed (Phase 0 ✓) | Dock bodies |
| Resizable split container | Build | Dock layout |
| Dock tabs and drag-to-dock | Build | All docks, 2D/3D switcher |
| Tree view (expand, select, drag to reparent) | Build | Scene tree, filesystem dock |
| Tooltip, context menu | Build on Feathers popover | Everywhere |
| Drag and drop | Build | Docks, asset browser |

Build in the order the editor needs them: splits and docks, then the tree, then text and number inputs. The widget gallery example shows every widget on one screen for comparison with Godot.

## Hub, CLI, MCP and plugins

These four reach the editor from outside its window, and all of them act through the command layer.

### Hub (like Unity Hub)

The Hub is a separate app in the same repo. It manages projects, installed editor versions and build-target modules, then launches the editor version each project pins.

- **Projects:** list, open, new from template, import.
- **Installs:** download editor builds listed in a version manifest (JSON, on GitHub Releases to start), verify them and unpack them side by side.
- **Modules:** Rust targets (`wasm32`, Android, iOS) via `rustup target add`.
- **Toolchain check:** Rust, linker and targets present.
- **Later:** template and plugin browser, news, self-update.

### Launch handoff

The Hub starts the editor with the project path, the Hub version and a session token. Started that way, the editor runs normally and can talk back to the Hub. Double-clicked by a user, it shows "Open this through Oddity Hub" with a launch button. Debug builds, headless commands and `--no-hub` skip the Hub.

### CLI

Both binaries parse arguments with `clap`.

```
oddity-hub install 0.3.0
oddity-hub list
oddity-hub new my_game --template 3d
oddity-hub open ./my_game                 # launches the pinned editor
oddity-hub doctor                         # checks Rust, linker, targets
oddity-editor ./my_game --no-hub          # direct start for development
oddity-editor run ./my_game
oddity-editor build ./my_game --target wasm32
oddity-editor plugin add <name>
oddity-editor mcp ./my_game               # MCP server over stdio
```

### MCP

The editor runs an MCP server built with `rmcp`, the official Rust MCP SDK.

- **Tools** are editor commands: list entities, inspect and set components, spawn, save, run.
- **Resources** expose the scene tree, project files and logs.
- **Transports:** stdio via `oddity-editor mcp`, and HTTP to attach to an editor that's already open.
- It runs headless, so agents and CI need no window.
- Later: a `bevy_remote` bridge into a running game, and the editor as an MCP client.

### Plugins

1. **v1, Rust crate plugins.** An `EditorPlugin` trait in `oddity_plugin_api` lets plugins add docks, menu items, commands, inspector widgets and asset importers. Projects list them as Cargo dependencies, so adding one means a rebuild.
2. **v2, WASM plugins.** Loaded at runtime with `wasmtime`: sandboxed, any language, no rebuild. Needed before a plugin marketplace.

## Bevy features to cover

These are the Bevy 0.20 areas the project depends on, in the order the phases need them.

| Bevy feature | Editor use | Phase |
| --- | --- | --- |
| ECS: components, resources, systems, queries, schedules | Everything | 0 |
| Assets: AssetServer, handles, file watcher, `embedded_asset` | Icons bundled in the binary; later the filesystem dock and hot reload | 0, 3 |
| Observers and events | Widget actions, selection changes, commands | 1 |
| Headless apps: `MinimalPlugins` + `ScheduleRunnerPlugin` | CLI, MCP, tests without a window | 1 |
| `Plugin` / `PluginGroup` | Editor structure; base of the plugin API | 1, 4 |
| Hierarchy: `ChildOf` / `Children` | Scene tree and UI structure | 2 |
| `bevy_ui`: Node, flexbox/grid, text, images | All editor panels | 2 |
| `bevy_ui_widgets` + `bevy_input_focus` | Widget base, keyboard focus, tab navigation | 2 |
| Input | Shortcuts (Ctrl+S, Ctrl+Z, Q/W/E/R tools) | 2 |
| `bevy_picking` | UI clicks, then viewport selection | 2, 3 |
| `bevy_reflect`: type registry, `Reflect` derive | Inspector and serialization | 3 |
| Scenes | Save and load | 3 |
| Cameras, render-to-texture, render layers | Viewport inside a dock; editor-only objects hidden from the game | 3 |
| Gizmos | Grid, selection outlines, transform handles | 3 |
| States | Edit mode vs play mode | 3 |
| `bevy_remote` | MCP bridge into a running game | Later |

Bevy 0.20 (released 8 October 2026) answers both questions this section used to leave open. It ships text editing (`EditableText`, plus Feathers `text_input` and `number_input`), and BSN, the new scene format, with the `bsn!` macro and scene patches. Feathers now also has menus, selects, list views, dialogs, scrollbars and pane containers.

## Roadmap

Six phases take the project from an empty window to a Hub that manages editor versions; it becomes a usable editor at the end of Phase 3.

&#91;embedded content: roadmap · 6 phases with exit criteria\]

No dates yet: set them once Phase 0 shows your real pace. MCP can move up to right after Phase 1, since it only needs the command layer.

## Risks, open questions and licensing

The biggest risk is Bevy's release pace; most mitigations come down to pinning versions and isolating what will change.

| Risk | Mitigation |
| --- | --- |
| Bevy breaks APIs about every three months | Pin the version; upgrade in a dedicated pass with the migration guide |
| AI suggestions target older Bevy APIs | Hand over the 0.20 notes at session start; feed compiler errors back; use the official `examples/` as reference |
| Text editing takes far longer than planned | Use Bevy 0.20's EditableText and Feathers text\_input; extend only where Godot's fields need more |
| Scene format changes (BSN) | Keep save/load behind `oddity_scene` so the format can be swapped |
| Public contracts (launch, plugin API) drift | Version them; test that older editors still launch from the current Hub |
| A solo project loses momentum | Every phase ends in something that runs; keep each phase small |

Open questions:

- [x] Final project name and logo: Oddity Engine by Hex0ddity Labs, wizard-cat mark
- [ ] 2D, 3D or both in the first release
- [x] Does Bevy ship a text input widget? Yes, 0.20 has EditableText and Feathers text and number inputs
- [x] State of BSN: shipped in Bevy 0.20 with `bsn!` and scene patches
- [ ] Long-term hosting for editor builds beyond GitHub Releases

Licensing: Godot's code and editor icons are MIT-licensed. Keep the original SVGs and Godot's LICENSE in `third_party/godot/` and credit them in `THIRD_PARTY_NOTICES.md`. The Godot name and logo are trademarks, so don't use them for branding.

## Scripting model

Game code is written in Rust. Godot's ideas (nodes, `get_node`, scripts with `ready`/`update`, signals) sit on top, and everything compiles down to plain Bevy, so every Bevy crate still works.

| What users see (Godot idea) | What Bevy runs underneath |
| --- | --- |
| Node in the scene tree | Entity + `Name` + `ChildOf` / `Children` |
| Node type (Sprite2D, Camera3D…) | A preset of components in a node registry; required components fill in the rest |
| Script with `ready` / `update` | A component plus systems generated by `#[script]` |
| `ctx.get_node("../Enemy")` | Path resolved by walking names in the hierarchy; returns a `NodeRef` |
| `#[signal]` + `ctx.emit` | An entity event, triggered on the emitting entity |
| `connect` (in code or the editor) | An observer on the emitter that calls a handler from the `#[handler]` table |
| Global signals | Buffered messages |
| Logic across many entities | Plain Bevy systems |

Rule of thumb: about *this* entity → script; between entities → signal; about *many* entities → system.

### Example

```rust
#[signal]
pub struct Died;

#[script]
impl Script for Player {
    fn update(&mut self, ctx: &mut Ctx, dt: f32) {
        ctx.transform.translation.x += self.speed * dt;

        let enemy = ctx.get_node("../Enemy");      // like Godot's get_node
        enemy.modify::<Health>(|h| h.0 -= 1);      // queued write

        if self.health <= 0 {
            ctx.emit(Died);                        // like emit_signal("died")
        }
    }
}

#[script]
impl Script for Enemy {
    fn ready(&mut self, ctx: &mut Ctx) {
        // like get_node("../Player").died.connect(_on_player_died)
        ctx.get_node("../Player").connect::<Died>(ctx.entity, "on_player_died");
    }

    #[handler]
    fn on_player_died(&mut self, _ctx: &mut Ctx, _signal: &Died) {
        self.celebrating = true;
    }
}

// Plain Bevy system for logic across many entities
fn despawn_dead(mut commands: Commands, q: Query<(Entity, &Health)>) {
    for (e, hp) in &q {
        if hp.0 <= 0 { commands.entity(e).despawn(); }
    }
}
```

### One frame, step by step

&#91;embedded content: one frame · script, get\_node, signal\]

Scripts read other nodes immediately, but their writes and emitted signals apply right after the script finishes, like Godot's `set_deferred`. A later Godot-exact dispatcher can make writes instant without changing any user code.

### Decisions and build order

- **Start with deferred writes.** Design `NodeRef` so an immediate dispatcher can replace the internals later.
- **Handlers are registered by name.** Rust can't look up methods at runtime, so `#[handler]` fills a name → function table used by the editor's Signals tab, MCP and the CLI.
- **Editor support:** unique sibling names, Copy Node Path, `%Unique` names, and warnings for broken paths and connections.
- **New Script** offers both a script (`ready`/`update`) and a plain system.
- **Per-project editor build:** opening a project compiles the editor and the game code together, so the inspector sees real Rust types.
- **Phase 1** adds the project build pipeline. **Phase 3** adds the node registry, `#[script]`, `get_node`, signals and the Signals tab, Attach Script and Play. **Phase 4** adds hot-patching of method bodies.
- **Hardest open piece:** scene instance overrides. Bevy 0.20 ships BSN with scene patches, so build instances and overrides on it rather than a format of our own.
