<p align="center">
  <img src="branding/oddity-cat-striped.svg" width="96" alt="Oddity Engine wizard cat">
</p>

# Oddity Engine

A Godot-style editor for [Bevy](https://bevy.org), written in Rust, by Hex0ddity Labs.

Godot's look, layout and icons on top of Bevy's ECS: Unity-style levels and
prefabs made of nodes, scripts with `ready` and `update`, and signals, all in
Rust and all compiled down to plain Bevy. A Unity-Hub-style launcher manages projects and editor
versions.

> **Status: Phase 2.** The editor opens in Godot 4.7's layout and look:
> Hierarchy and FileSystem on the left, the 3D/2D viewport with Godot's
> toolbar and the bottom panel in the middle, the Inspector on the right. The
> Hierarchy shows a (demo) level's nodes: select, filter, show/hide, add
> (`Ctrl+A`) and delete (`Delete`), all undoable and listed in the History
> dock. The Inspector shows the selected node's components and values. The
> FileSystem dock shows the open project's files. Editing values,
> levels/prefabs on disk and the 3D view itself come in Phase 3.

## Build and run

You need Rust **1.97.1 or newer** (`rustup update`), because Bevy 0.20 requires it.

```sh
cargo editor          # runs the editor with fast dev builds (dynamic linking)
cargo run -p oddity-editor --release
cargo test --workspace
```

The first build compiles Bevy and takes several minutes; later builds are quick.

To save a screenshot of the editor and quit (handy for reviews and CI):

```sh
ODDITY_SCREENSHOT=editor.png cargo editor            # macOS / Linux
$env:ODDITY_SCREENSHOT="editor.png"; cargo editor    # Windows PowerShell
```

- **Windows:** works out of the box. `.cargo/config.toml` uses `rust-lld`, which
  ships with Rust, for faster linking.
- **Linux:** install `clang`, `lld` and Bevy's
  [Linux dependencies](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md)
  (on Ubuntu: `sudo apt install clang lld pkg-config libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev`).
- **macOS:** works out of the box.

## Repository layout

```
Oddity-engine/
├── apps/editor/           # oddity-editor: window chrome, docks, layout (see docs/architecture.md)
├── crates/
│   ├── oddity_core/       # commands, undo/redo, selection, level root (headless)
│   ├── oddity_icons/      # Godot icons, @icons, brand marks, UI glyphs: SVG → pixels at any scale
│   ├── oddity_project/    # oddity.toml: create, load, save, find
│   └── oddity_widgets/    # theme, fonts, docks, splits, tree view, icon buttons
├── docs/
│   ├── architecture.md    # how the pieces fit, and the rules that keep them fitting
│   └── conventions.md     # code style, comments, tests
├── branding/              # logo, wordmark, colours (see branding/README.md)
├── third_party/           # Godot 4.7.2 editor icons, @icons, each with its licence
└── THIRD_PARTY_NOTICES.md
```

Crates are added as each phase needs them: `oddity_scene`, `oddity_inspector`,
`oddity_viewport`, `oddity_mcp`, `oddity_plugin_api`, `oddity_launch` and the Hub.

## Editor shortcuts

| Keys | Action |
| --- | --- |
| `Ctrl+Z` / `Ctrl+Shift+Z` or `Ctrl+Y` | Undo / redo |
| `Ctrl+A` | Add a node under the selection |
| `Delete` | Delete the selected nodes |
| Click / `Ctrl`+click / `Shift`+click | Select / toggle / select range in the Hierarchy |
| `Q` / `W` / `E` / `R` | Select / move / rotate / scale tool |
| `T` / `Y` | Local space / snapping on or off |
| `Ctrl+Shift+F11` | Distraction-free mode (viewport only) |

`Cmd` instead of `Ctrl` on macOS. Drag the gaps between docks to resize them.
Click a step in the History dock to undo or redo back to it.

## Commands and undo

Every change goes through an `EditorCommand`, so undo/redo works for the UI,
shortcuts, CLI, MCP and plugins alike:

```rust
use oddity_core::{EditorWorldExt, commands::{SpawnEntity, RenameEntity}};

let spawn = SpawnEntity::new(world, "Player");
let player = spawn.entity();
world.execute(spawn)?;
world.execute(RenameEntity::new(player, "Hero"))?;
world.undo()?;                                   // Ctrl+Z in the editor

// The same by name, with JSON — what the CLI and MCP use:
world.run_command("component.set", json!({
    "entity": player.to_bits(), "component": "Transform",
    "path": "translation.x", "value": 3.0,
}))?;
```

Deleting disables entities instead of despawning them, so undo restores the
very same entities; they are despawned once the step leaves the history.

## Levels and prefabs

Oddity uses Unity's model with Unreal-style isolated editing:

| Concept | What it is | How you edit it |
| --- | --- | --- |
| **Level** (`.level`) | The world you play: terrain, lights, placed objects | In the main viewport; Play runs the main level |
| **Prefab** (`.prefab`) | A reusable object: Player, Enemy, Door | Opened in isolation in its own tab, breadcrumb `Level ▸ Enemy` |
| **Instance** | A prefab placed in a level | Overridden values show in bold; Apply or Revert |

Both are BSN files underneath. (Arrives in Phase 3.)

## Projects

A project is a folder with an `oddity.toml` (Oddity's `project.godot`).
Open one with `cargo editor -- path/to/game`; any folder inside the project works.

## Using the icons

```rust
use oddity_icons::{Icon, NodeColor, UiIcon};

commands.spawn(UiIcon::new(Icon::godot("Node3D"), 16.0));                         // Godot's icon
commands.spawn(UiIcon::new(Icon::at("hammer").tinted(NodeColor::Node2D), 16.0));  // @icons, tinted blue
commands.spawn(UiIcon::new(Icon::oddity_cat(), 64.0));                            // brand
```

Icons are embedded as SVG and rendered at the exact pixel size for the
screen's scale factor, so they stay sharp on high-DPI displays.

## Bevy is the seed

Oddity is built from **Bevy 0.20.0**, pinned exactly (`=0.20.0`). We do not
track upstream Bevy releases. When Oddity needs to change engine internals,
Bevy gets forked into this repo (for example `engine/bevy`, as a git subtree)
and the workspace points at that copy instead of crates.io.

## Theme

`oddity_widgets::ThemeConfig` ports Godot's rule for building its editor theme
from a base colour, an accent colour and a contrast value, and feeds the
result into Bevy Feathers' theme tokens. The default is Godot's dark grey with
the Oddity hot-pink accent; `ThemeConfig::godot()` gives Godot's stock blue.

## Roadmap

The full roadmap, with diagrams, is in [roadmap/](roadmap/README.md).

| Phase | What | Done when |
| --- | --- | --- |
| **0 · Foundations** ✓ | Workspace on Bevy 0.20, fast compiles, CI, icons, theme, fonts | Editor window opens and CI is green |
| **1 · Core** ✓ | Command registry, undo/redo, selection, project file, headless mode | Commands run headless in tests, with undo |
| **2 · Widgets and shell** ✓ | Godot 4.7 look; docks, splits, tree, filters; Hierarchy, Inspector, History, FileSystem | The layout looks like Godot's |
| 3 · Editing | Editable inspector, 3D view, gizmos, picking, levels, prefab mode, scripts, play | Make, save, reopen and play a level; edit a prefab in isolation |
| 4 · CLI, MCP, plugins | Editor CLI, MCP server, `EditorPlugin` API | An agent and a plugin both edit a scene |
| 5 · Hub | Launch handoff, Hub CLI and UI, version manifest | Hub installs two versions and opens each project |

## Credits

Editor icons from [Godot Engine](https://godotengine.org) (MIT) and
[@icons](https://github.com/Voxybuns/at-icons) by Voxybuns (MIT). Fonts: Inter,
Space Grotesk, Geist Mono (SIL OFL). See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

Oddity Engine is not affiliated with or endorsed by the Godot Engine project.
