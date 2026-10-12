<p align="center">
  <img src="branding/oddity-cat-striped.svg" width="96" alt="Oddity Engine wizard cat">
</p>

# Oddity Engine

A Godot-style editor for [Bevy](https://bevy.org), written in Rust, by Hex0ddity Labs.

Godot's look, layout and icons on top of Bevy's ECS: a scene tree of nodes,
scripts with `ready` and `update`, and signals, all in Rust and all compiled
down to plain Bevy. A Unity-Hub-style launcher manages projects and editor
versions.

> **Status: Phase 0.** The workspace, theme, fonts and icons are in place and
> the editor window opens. Nothing is editable yet. See the roadmap below.

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
├── apps/
│   └── editor/            # oddity-editor binary
├── crates/
│   ├── oddity_icons/      # Godot icons, @icons and brand marks, SVG → pixels at any scale
│   └── oddity_widgets/    # our "Feathers": Godot palette, brand colours, fonts
├── branding/              # logo, wordmark, colours (see branding/README.md)
├── third_party/
│   ├── godot/             # Godot 4.7.2 editor icons + MIT licence
│   └── at-icons/          # @icons by Voxybuns + MIT licence
└── THIRD_PARTY_NOTICES.md
```

Crates are added as each phase needs them: `oddity_core` (commands, undo/redo),
`oddity_project`, `oddity_scene`, `oddity_inspector`, `oddity_viewport`,
`oddity_mcp`, `oddity_plugin_api`, `oddity_launch` and the Hub.

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

| Phase | What | Done when |
| --- | --- | --- |
| **0 · Foundations** | Workspace on Bevy 0.20, fast compiles, CI, icons, theme, fonts | Editor window opens and CI is green |
| 1 · Core | Command registry, undo/redo, selection, project file, headless mode | Commands run headless in tests, with undo |
| 2 · Widgets and shell | Docks, splits, tabs, tree view on top of Feathers | Four-dock layout looks like Godot's |
| 3 · Editing | Scene tree, inspector, viewport, gizmos, picking, save/load, play | Make, save, reopen and play a scene |
| 4 · CLI, MCP, plugins | Editor CLI, MCP server, `EditorPlugin` API | An agent and a plugin both edit a scene |
| 5 · Hub | Launch handoff, Hub CLI and UI, version manifest | Hub installs two versions and opens each project |

## Credits

Editor icons from [Godot Engine](https://godotengine.org) (MIT) and
[@icons](https://github.com/Voxybuns/at-icons) by Voxybuns (MIT). Fonts: Inter,
Space Grotesk, Geist Mono (SIL OFL). See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

Oddity Engine is not affiliated with or endorsed by the Godot Engine project.
