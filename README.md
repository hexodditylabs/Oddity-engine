<p align="center">
  <a href="https://hexodditylabs.dev">
    <img src="branding/oddity-cat-striped.svg" width="96" alt="Oddity Engine wizard cat">
  </a>
</p>

<h1 align="center">Oddity Engine</h1>

<p align="center">
  <a href="https://hexodditylabs.dev"><img src="https://img.shields.io/badge/Oddity-Engine-FF2D55?style=for-the-badge&labelColor=0A0A0A" alt="Oddity Engine"></a>
  <a href="https://hexodditylabs.dev"><img src="https://img.shields.io/badge/by-Hex0ddity_Labs-FF5A5F?style=for-the-badge&labelColor=0A0A0A" alt="by Hex0ddity Labs"></a>
</p>

<p align="center">
  A game engine and editor written in Rust, by <a href="https://hexodditylabs.dev">Hex0ddity Labs</a>.
</p>

Levels and prefabs made of nodes, Unity-style scripts in Rust with callbacks
and signals, and an editor where every action can be undone, scripted or
driven by an AI agent. A Hub manages your projects and editor versions.

> **Status: Phase 3 has started.** The editor opens with its full layout:
> Hierarchy, FileSystem and History on the left, the 3D/2D viewport with its
> toolbar in the middle, the Inspector on the right, and the bottom panel
> underneath. The Hierarchy shows a demo level's nodes: select, filter,
> show/hide, add (`Ctrl+A`) and delete (`Delete`), all undoable and listed in
> History. The Inspector shows the selected node's components and values. The
> FileSystem dock shows the open project's folders; the Files tab in the bottom
> panel shows the chosen folder's contents as a grid. Editing values, saving
> levels and prefabs, and the 3D view itself are being built now.

## Build and run

You need Rust **1.97.1 or newer**. Install it with [rustup](https://rustup.rs)
(`rustup update` if you have it already). Rust from a Linux distribution's
package manager is usually too old.

```sh
cargo editor                           # run the editor (fast dev builds)
cargo editor -- samples/shooter        # run it with the sample project open
cargo editor -- mcp samples/shooter    # no window: an MCP server on stdin/stdout
cargo run -p oddity-editor --release   # an optimised build
```

The first build compiles the whole engine and takes several minutes; later
builds are quick.

- **Windows:** works out of the box.
- **Linux:** install the linker and system libraries. On Ubuntu:
  `sudo apt install clang lld pkg-config libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev`
- **macOS:** works out of the box.

### Checks

These must pass before a change goes in; CI runs the same:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask layers                     # the crate layer rules
```

To save a screenshot of the editor and quit (handy for reviews and CI):

```sh
ODDITY_SCREENSHOT=editor.png cargo editor            # macOS / Linux
$env:ODDITY_SCREENSHOT="editor.png"; cargo editor    # Windows PowerShell
```

## Projects

A project is a folder with an `oddity.toml`. Open one with
`cargo editor -- path/to/game`; any folder inside the project works.
[samples/shooter](samples/shooter/) is the small top-down shooter used to test
the engine as it grows.

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

Every change goes through an `EditorCommand`, so undo and redo work the same
for the UI, shortcuts, the CLI, MCP and plugins:

```rust
use oddity_core::{EditorWorldExt, commands::{SpawnEntity, RenameEntity}};

let spawn = SpawnEntity::new(world, "Player");
let player = spawn.entity();
world.execute(spawn)?;
world.execute(RenameEntity::new(player, "Hero"))?;
world.undo()?;                                   // Ctrl+Z in the editor

// The same by name, with JSON: what the CLI and MCP use.
world.run_command("component.set", json!({
    "entity": player.to_bits(), "component": "Transform",
    "path": "translation.x", "value": 3.0,
}))?;
```

Deleting hides entities instead of destroying them, so undo restores the very
same entities; they are destroyed once the step leaves the history.

## Levels and prefabs

| Concept | What it is | How you edit it |
| --- | --- | --- |
| **Level** (`.level`) | The world you play: terrain, lights, placed objects | In the main viewport; Play runs the main level |
| **Prefab** (`.prefab`) | A reusable object: Player, Enemy, Door | Opened on its own in a tab, with a breadcrumb `Level ▸ Enemy` |
| **Instance** | A prefab placed in a level | Changed values show in bold; Apply or Revert |

Both are JSON files. (Arriving in Phase 3.)

## Repository layout

```
Oddity-engine/
├── apps/editor/           # oddity-editor: window chrome, docks, layout
├── crates/
│   ├── oddity_core/       # commands, undo/redo, selection, level root (headless)
│   ├── oddity_icons/      # editor icons and brand marks, rendered sharp at any size
│   ├── oddity_plugin_api/ # the door into the editor: modes, commands, dock tabs, menus
│   ├── oddity_project/    # oddity.toml: create, load, save, find
│   └── oddity_widgets/    # theme, fonts, docks, splits, tree view, file grid, buttons
├── tools/xtask/           # cargo xtask layers: the crate layer check
├── samples/shooter/       # the sample game
├── docs/                  # architecture and conventions
├── roadmap/               # the roadmap and the design specs
├── branding/              # logo, wordmark, colours (see branding/README.md)
├── third_party/           # icon sets, each with its licence
└── THIRD_PARTY_NOTICES.md
```

Crates are added as each phase needs them. Every crate sits in a layer
(runtime, tooling, extension, app), and every feature is its own set of
crates; see [roadmap/modularity.md](roadmap/modularity.md).

## Roadmap

The full roadmap is in [roadmap/](roadmap/README.md).

| Phase | What | Done when |
| --- | --- | --- |
| **0 · Foundations** ✓ | Workspace, fast compiles, CI, icons, theme, fonts | The editor window opens and CI is green |
| **1 · Core** ✓ | Command registry, undo/redo, selection, project file, headless mode | Commands run headless in tests, with undo |
| **2 · Widgets and shell** ✓ | The editor's look; docks, splits, tree, filters; Hierarchy, Inspector, History, FileSystem | The full editor layout is in place |
| 3 · Editing | Viewport, gizmos, levels and prefabs, scripting, Play, physics | Make, save, reopen and play a level; edit a prefab on its own |
| 4 · Game features | Assets, materials, animation, game UI, saving, export, Steam, 2D, navigation, particles | A small game can be shipped on Steam |
| 5 · Multiplayer | Host/join, synced nodes and fields, RPCs, QUIC and Steam transports | Two players play a co-op level over Steam |
| 6 · CLI, MCP, plugins | Editor CLI, MCP server, `EditorPlugin` API | An agent and a plugin both edit a level |
| 7 · Hub | Launch handoff, Hub CLI and UI, version manifest | The Hub installs two versions and opens each project |

## Credits

Made by [Hex0ddity Labs](https://hexodditylabs.dev). Third-party icons and
fonts are listed, with their licences, in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
