# Oddity Engine roadmap

*Updated 14 October 2026 · Phases 0–2 built · Phase 3 is next*

Oddity Engine, by Hex0ddity Labs, is a Godot-style editor for Bevy 0.20,
written in Rust, with a Unity-Hub-style launcher on top. It copies Godot's look,
layout and icons; underneath it is Bevy's ECS, and everything a game uses
compiles down to plain Bevy.

![The six phases, with Phases 0 to 2 built](images/phases.svg)

**Contents**

1. [Where we are](#where-we-are)
2. [Vision and principles](#vision-and-principles)
3. [Architecture](#architecture)
4. [Repository layout](#repository-layout)
5. [Phases in detail](#phases-in-detail)
6. [Levels, prefabs and instances](#levels-prefabs-and-instances)
7. [Scripting model](#scripting-model)
8. [Hub, CLI, MCP and plugins](#hub-cli-mcp-and-plugins)
9. [Widget library](#widget-library)
10. [Bevy features we depend on](#bevy-features-we-depend-on)
11. [Prerequisites](#prerequisites)
12. [Risks, open questions and licensing](#risks-open-questions-and-licensing)

---

## Where we are

The editor opens in Godot 4.7's layout and look, with real docks working on a
demo level. Every edit goes through undoable commands.

![The Oddity editor at the end of Phase 2](images/editor-phase-2.png)

Side by side with Godot 4.7 at the same size. Sizes and colours were measured
from Godot, and a test keeps the theme's colours matched to it.

![Godot 4.7 on top, Oddity Engine underneath](images/godot-vs-oddity.png)

What works today:

- **Hierarchy:** the level's node tree with Godot's icons and relationship
  lines. Select (click, Ctrl+click, Shift+click), collapse, filter, show/hide
  with the eye, add (Ctrl+A) and delete (Delete).
- **Inspector:** the selected node's components in Godot's category bands,
  values read through reflection: Position, Rotation in degrees, Scale as x/y/z
  fields. Filterable. Read-only until Phase 3.
- **History:** every undo step, with undone steps dimmed. Click a step to jump
  back or forward to it.
- **FileSystem:** the open project's files, with Godot's icons and blue folders.
- **Viewport toolbar:** Select, Move, Rotate, Scale, local space and snap, on
  Godot's keys (Q, W, E, R, T, Y). The 3D/2D switch and distraction-free mode
  (Ctrl+Shift+F11). The 3D view itself arrives in Phase 3.
- **Window:** custom title bar with menus, level tab and picker, and a
  jackdaw-style play bar; resize from any edge.
- **Underneath:** command registry with undo/redo and a dirty flag, selection,
  `oddity.toml` projects, headless mode for tests, CLI and MCP.

## Vision and principles

- **Godot look, Bevy model.** Godot's dock layout, dark theme and MIT-licensed
  icons, with entities and components underneath. The Bevy you know keeps working.
- **One command layer.** Every action is a command. Menus, shortcuts, the CLI,
  MCP and plugins all call the same commands, which gives undo/redo everywhere.
- **Native Bevy UI.** Our own widget layer on Bevy's `bevy_ui_widgets` and
  Feathers, themed with Godot's palette.
- **Rust is the scripting language.** Godot's ideas (nodes, `get_node`,
  `ready`/`update`, signals) on top; no built-in code editor. Scripts open in
  VS Code or Zed, which bring rust-analyzer.
- **The Hub for users, never required for development.** Debug builds,
  headless commands and `--no-hub` skip it.
- **Bevy 0.20 is the seed.** Pinned to exactly `0.20.0`. Each project pins its
  editor and Bevy version, and upgrades are deliberate.
- **Small steps.** Every phase ends in something that runs.

## Architecture

Two binaries share one workspace, and every way of changing a project goes
through the command layer in `oddity_core`.

![Every way in goes through one command layer](images/architecture.svg)

The Hub reads a project's `oddity.toml`, then starts the matching editor
through the `oddity_launch` handoff. Inside the editor, the UI, CLI, MCP server
and plugins never touch the World directly; they run commands, so undo/redo,
scripting and AI control are one feature. Headless mode is the same editor with
`MinimalPlugins` and no window, used by the CLI, MCP and tests.

The level being edited lives under a `LevelRoot` entity in the same World as the
editor UI; anything outside it is editor, not level. Deleting disables entities
instead of despawning them, so undo brings back the very same entities.

## Repository layout

One Cargo workspace holds both apps and every crate, so the Hub and editor share
the widgets, project format and launch contract. Crates are created when a phase
needs them.

```
Oddity-engine/
├── Cargo.toml                 workspace root, pins bevy = "=0.20.0"
├── .cargo/config.toml         fast linking, `cargo editor` alias
├── .github/workflows/ci.yml   fmt, clippy, tests, Windows build
├── docs/                      architecture and code conventions
├── roadmap/                   this roadmap and its diagrams
├── branding/                  cat, wordmark, Hex0ddity mark, brand board
├── crates/
│   ├── oddity_icons/          Godot + @icons + brand SVGs, sharp at any scale   ✓
│   ├── oddity_widgets/        theme, fonts, docks, tree, fields, buttons         ✓
│   ├── oddity_core/           commands, undo/redo, selection, level root         ✓
│   ├── oddity_project/        oddity.toml format                                 ✓
│   ├── oddity_scene/          levels and prefabs on top of BSN                   Phase 3
│   ├── oddity_viewport/       editor camera, grid, gizmos, picking               Phase 3
│   ├── oddity_script/         #[script], get_node, signals, handler table        Phase 3
│   ├── oddity_mcp/            MCP server: commands → tools                       Phase 4
│   ├── oddity_plugin_api/     EditorPlugin trait (public contract)               Phase 4
│   ├── oddity_launch/         Hub ↔ editor handoff (public contract)             Phase 5
│   └── oddity_hub/            installs, version manifest, project registry       Phase 5
├── apps/
│   ├── editor/                oddity-editor binary                               ✓
│   ├── hub/                   oddity-hub binary: UI + CLI                        Phase 5
│   └── sandbox/               a test game to edit                                Phase 3
├── templates/                 2d/ and 3d/ project templates                      Phase 5
├── third_party/               Godot icons and @icons, with their MIT licences    ✓
└── THIRD_PARTY_NOTICES.md
```

Two crates are public contracts: `oddity_launch` (older editors must still
launch from a newer Hub) and `oddity_plugin_api` (third-party plugins depend on
it). Change them rarely and version them.

## Phases in detail

No dates: they get set once the pace of Phase 3 is known. MCP can move up to
follow Phase 1, since it only needs the command layer.

### Phase 0 · Foundations ✓

*Done when an empty editor window opens and CI is green.*

- [x] Cargo workspace pinned to Bevy `=0.20.0`; fast linking; `cargo editor`
- [x] CI: fmt, clippy (`-D warnings`), tests, Windows build
- [x] Icons: 1,024 Godot icons and 642 @icons embedded as SVG, rendered by
      resvg at the exact size and screen scale; @icons tinted per node family
- [x] Theme: Godot 4.7's theme rules (base, accent, contrast) ported, with the
      Oddity hot-pink accent, fed into Feathers' tokens
- [x] Fonts: Inter, Space Grotesk, Geist Mono

### Phase 1 · Core ✓

*Done when commands run headless in tests, with undo.*

- [x] `EditorCommand` and `History`: undo/redo, a 500-step limit, a dirty flag
- [x] Commands: spawn, delete (disable), rename, reparent, set field, add and
      remove component, batch, selection
- [x] `CommandRegistry` with JSON arguments, ready for the CLI and MCP
- [x] Reflection to JSON for any component
- [x] `oddity.toml` projects: create, load, find, version check
- [x] `LevelRoot`: what is level and what is editor

### Phase 2 · Widgets and shell ✓

*Done when the layout looks like Godot's.*

- [x] Custom title bar: minimise, maximise, close, drag, edge resize
- [x] Top bar after jackdaw: menus, level tabs, level picker, play controls
- [x] Docks and Godot's bottom panel; resizable splits; distraction-free mode
- [x] Tree view: Godot's rows, relationship lines, row buttons, filtering
- [x] Search fields, tool buttons with toggle state, hover feedback
- [x] Hierarchy, Inspector (read-only), History, FileSystem, Output
- [x] Viewport toolbar state on Godot's keys
- [x] Godot 4.7 look, measured from screenshots and checked by a test

### Phase 3 · Editing (next)

*Done when you can make, save, reopen and play a level, and edit a prefab in
isolation.* This is where Oddity becomes a usable editor.

**Inspector**
- [ ] Editable fields: Feathers number and text inputs, checkboxes, dropdowns;
      dragging a number scrubs it; every change is one undo step
- [ ] Add Component and Remove Component
- [ ] Instance overrides shown in bold, with Apply and Revert

**Viewport**
- [ ] 3D view rendered to a texture inside the dock, with Godot's grid and sky
- [ ] Editor camera: orbit, pan, fly; frame selection (F)
- [ ] Click to select (`bevy_picking`), box select
- [ ] Gizmos for move, rotate and scale, honouring local space and snapping
- [ ] Editor-only objects on their own render layer, hidden from the game
- [ ] 2D view with Godot's rulers and canvas

**Levels and prefabs**
- [ ] Save and load `.level` and `.prefab` files (BSN) through `oddity_scene`
- [ ] Level tabs and the level picker work for real; unsaved-changes prompt
- [ ] Prefab mode: its own tab, an isolated world, a breadcrumb
- [ ] Instances: place a prefab in a level, override values, Apply / Revert
- [ ] FileSystem: watch for changes, drag a prefab into the viewport or hierarchy

**Nodes**
- [ ] Node registry: node types as component presets, each with its Godot icon
- [ ] Add Node dialog with search and favourites
- [ ] Rename in place, drag to reparent, context menu, Copy Node Path,
      `%Unique` names

**Scripting**
- [ ] Per-project editor build: the editor compiles together with the game code
- [ ] `#[script]` with `ready` and `update`; `Ctx`; `get_node` returning `NodeRef`
- [ ] `#[signal]`, `ctx.emit`, `connect`, the `#[handler]` table
- [ ] Signals dock: connect a signal to a handler by name
- [ ] Attach Script / New Script; "Open in VS Code / Zed"; clickable compiler
      errors

**Play**
- [ ] Play, pause, stop, reload: edit state vs play state, embedded or windowed
- [ ] Output dock shows the game's log

### Phase 4 · CLI, MCP and plugins

*Done when an agent and a sample plugin both edit a level.*

- [ ] Editor CLI with `clap`: `run`, `build --target`, `plugin add`
- [ ] MCP server with `rmcp`: tools are editor commands; resources are the
      level tree, project files and logs; stdio and HTTP; works headless
- [ ] `EditorPlugin` API v1 in `oddity_plugin_api`: docks, menu items, commands,
      inspector widgets, asset importers; plugins are Cargo dependencies
- [ ] Hot-patching of script method bodies

### Phase 5 · Hub

*Done when the Hub installs two editor versions and opens a project in each.*

- [ ] Launch handoff contract in `oddity_launch`; the editor refuses to start
      without the Hub except in dev, headless and `--no-hub`
- [ ] Hub CLI: `new`, `open`, `install`, `list`, `doctor`
- [ ] Hub UI: projects, installs, modules (Rust targets), toolchain check
- [ ] Version manifest on GitHub Releases; verified downloads, side-by-side installs
- [ ] Project templates (2D, 3D)

### After Phase 5

- Prefab variants (a Goblin based on Enemy) and nested prefabs
- WASM plugins loaded at runtime with `wasmtime`, then a plugin browser
- A `bevy_remote` bridge into the running game for MCP; the editor as an MCP client
- An immediate (Godot-exact) script dispatcher, if deferred writes ever get in the way
- Asset previews, import settings, animation and shader docks
- Hub extras: templates and plugin browser, news, self-update

## Levels, prefabs and instances

Oddity uses Unity's model with Unreal-style isolated editing instead of Godot's
single "scene" concept. Decided 14 October 2026.

![A level holds instances; a prefab is edited on its own](images/levels-prefabs.svg)

| Concept | What it is | How you edit it |
| --- | --- | --- |
| **Level** (`.level`) | The world you play: terrain, lights, placed objects. Unity calls it a scene. | In the main viewport. Play runs the main level. |
| **Prefab** (`.prefab`) | A reusable object: Player, Enemy, Door. | Double-click to open in isolation: its own tab, an empty world with only the prefab, and a breadcrumb `Level ▸ Enemy`. Like Unity's Prefab Mode and Unreal's Blueprint editor. |
| **Instance** | A prefab placed in a level. | Change values in the Inspector. Changed values show in bold; Apply sends them to the prefab, Revert drops them. |

- **Underneath, both are BSN files.** This is an editor decision, not an engine
  one, so it stays cheap to change. Bevy 0.20's scene patches carry the overrides.
- **Unchanged by this:** the node tree inside a level or prefab, `get_node`,
  scripts with `ready`/`update`, and signals.
- **Project file:** `[editor] main_level = "levels/main.level"` sets what Play runs.

## Scripting model

Game code is written in Rust. Godot's ideas sit on top, and everything compiles
down to plain Bevy, so every Bevy crate still works.

| What you see (Godot idea) | What Bevy runs underneath |
| --- | --- |
| Node in the Hierarchy | Entity + `Name` + `ChildOf` / `Children` |
| Node type (Sprite2D, Camera3D…) | A preset of components in the node registry; required components fill in the rest |
| Script with `ready` / `update` | A component plus systems generated by `#[script]` |
| `ctx.get_node("../Enemy")` | A path resolved by walking names in the hierarchy; returns a `NodeRef` |
| `#[signal]` + `ctx.emit` | An entity event, triggered on the emitting entity |
| `connect` (in code or the editor) | An observer on the emitter that calls a handler from the `#[handler]` table |
| Global signals | Buffered messages |
| Logic across many entities | Plain Bevy systems |

Rule of thumb: about *this* entity → script; between entities → signal; about
*many* entities → system.

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

![Scripts read now; writes and signals land right after](images/scripting-frame.svg)

Scripts read other nodes immediately, but their writes and emitted signals
apply right after the script finishes, like Godot's `set_deferred`. A later
Godot-exact dispatcher can make writes instant without changing user code.

### Decisions

- **Deferred writes first.** `NodeRef` is designed so an immediate dispatcher
  can replace its internals later.
- **Handlers are registered by name.** Rust can't look up methods at runtime,
  so `#[handler]` fills a name → function table used by the Signals dock, MCP
  and the CLI.
- **Editor support:** unique sibling names (already enforced when adding
  nodes), Copy Node Path, `%Unique` names, and warnings for broken paths and
  connections.
- **No built-in script editor.** New Script offers a script or a plain system,
  and opens it in VS Code or Zed.
- **Per-project editor build,** so the Inspector sees the game's real Rust types.
- **Hardest open piece:** instance overrides with Apply / Revert, built on BSN
  scene patches rather than a format of our own.

## Hub, CLI, MCP and plugins

These reach the editor from outside its window, and all of them act through
the command layer.

**Hub (like Unity Hub).** A separate app in the same repo. It manages projects,
installed editor versions and build-target modules, then launches the editor
version each project pins.

**Launch handoff.** The Hub starts the editor with the project path, the Hub
version and a session token. Double-clicked directly, the editor shows "Open
this through Oddity Hub" with a launch button. Debug builds, headless commands
and `--no-hub` skip the Hub.

**CLI.** Both binaries parse arguments with `clap`:

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

**MCP.** The editor runs an MCP server built with `rmcp`. Tools are editor
commands (list entities, inspect and set components, spawn, save, run);
resources expose the level tree, project files and logs. Stdio for agents and
CI, HTTP to attach to an editor that's already open. It runs headless.

**Plugins.** v1: Rust crate plugins through the `EditorPlugin` trait, listed as
Cargo dependencies (adding one means a rebuild). v2: WASM plugins with
`wasmtime`, sandboxed and loaded at runtime, needed before a marketplace.

## Widget library

Our own take on `bevy_feathers`: Feathers' controls, themed with Godot's
palette, plus the editor-shaped widgets Feathers doesn't have.

| Widget | Source | Status |
| --- | --- | --- |
| Button, checkbox, radio, slider, toggle, colour picker | Feathers, themed | ✓ |
| Text input, number input | Feathers, themed | search fields ✓; number fields in Phase 3 |
| Menu, select, list view, dialog, scrollbar | Feathers, themed | Phase 3 |
| Docks, tabs and Godot's bottom panel | Ours | ✓ |
| Resizable splits | Ours | ✓ |
| Tree view (lines, row buttons, filter) | Ours | ✓; drag to reparent in Phase 3 |
| Tool buttons and toggles, hover feedback | Ours | ✓ |
| Tooltip, context menu | Ours, on Feathers popover | Phase 3 |
| Drag and drop | Ours | Phase 3 |

## Bevy features we depend on

| Bevy feature | Editor use | Phase |
| --- | --- | --- |
| ECS: components, resources, systems, queries, schedules | Everything | 0 ✓ |
| Assets, `embedded_asset` | Fonts and icons inside the binary; later the FileSystem watcher | 0 ✓, 3 |
| Observers and events | Widget actions, tree events, selection | 1 ✓ |
| Headless apps (`MinimalPlugins`) | CLI, MCP, tests without a window | 1 ✓ |
| Entity disabling | Undoable delete | 1 ✓ |
| Hierarchy: `ChildOf` / `Children` | The node tree and the UI | 2 ✓ |
| `bevy_ui`, BSN (`bsn!`), Feathers | Every panel | 2 ✓ |
| `bevy_input_focus`, `EditableText` | Keyboard focus, search fields, shortcuts | 2 ✓ |
| `bevy_picking` | UI clicks ✓, then viewport selection | 2 ✓, 3 |
| `bevy_reflect` | Inspector ✓, saving, editable fields | 2 ✓, 3 |
| BSN scenes and scene patches | Levels, prefabs, instance overrides | 3 |
| Cameras, render-to-texture, render layers | Viewport inside a dock; editor-only objects | 3 |
| Gizmos | Grid, selection outlines, transform handles | 3 |
| States | Edit mode vs play mode | 3 |
| `bevy_remote` | MCP bridge into the running game | Later |

## Prerequisites

| Need | Why |
| --- | --- |
| Rust 1.97.1 or newer (`rustup update`) | Bevy 0.20 requires it |
| VS Code, Zed or RustRover with rust-analyzer | Completion and inline errors; scripts open there |
| A fast linker (configured in `.cargo/config.toml`) | Bevy's default build times are slow |
| Godot 4.7 installed | Side-by-side visual reference; measure, don't eyeball |
| Bevy 0.20 release notes and migration guide | AI models may know older APIs; hand these over at the start of a session |

Knowledge: comfortable Rust (ownership, traits, generics), Bevy ECS basics
(components, systems, queries, observers), and flexbox for Bevy UI layout.

## Risks, open questions and licensing

| Risk | Mitigation |
| --- | --- |
| Bevy breaks APIs about every three months | Pinned to `=0.20.0`; upgrade only in a dedicated pass with the migration guide |
| AI suggestions target older Bevy APIs | Hand over the 0.20 notes at session start; feed compiler errors back; use Bevy's examples as reference |
| BSN keeps changing | Keep save/load behind `oddity_scene` so the format can be swapped |
| Instance overrides are hard | Build on BSN scene patches; ship Apply/Revert before variants and nesting |
| Per-project editor builds are slow | Dynamic linking in dev; hot-patching in Phase 4 |
| Public contracts (launch, plugin API) drift | Version them; test that older editors still launch from the current Hub |
| A solo project loses momentum | Every phase ends in something that runs; keep each phase small |

**Open questions**

- 2D, 3D or both in the first release
- Long-term hosting for editor builds beyond GitHub Releases
- Which code editor "Open script" defaults to, and how it's chosen

**Settled:** name and logo (Oddity Engine by Hex0ddity Labs, wizard-cat mark);
Bevy 0.20 has text editing (`EditableText`, Feathers inputs) and BSN with
scene patches; levels and prefabs instead of scenes.

**Licensing.** Godot's code and editor icons are MIT-licensed; the SVGs and
Godot's licence live in `third_party/godot/` and are credited in
`THIRD_PARTY_NOTICES.md`. @icons by Voxybuns is MIT-licensed too. The Godot name
and logo are trademarks, so they are never used for Oddity's branding.
