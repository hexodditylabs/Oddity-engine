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
`EditorCommand` through `world.execute(..)` or `world.run_command("id", json)`.
That single rule is what gives undo/redo everywhere, lets an AI agent drive
the editor, and keeps "unsaved changes" honest.

Reading is free: any system may query the level. Docks refresh by reading the
world every frame (cheap, and always correct after undo or an MCP edit)
instead of listening for change notifications.

## Crates

| Crate | What it owns | Depends on |
| --- | --- | --- |
| `oddity_core` | Commands, undo/redo history, selection, the level root, the command registry (JSON, for CLI/MCP). Headless: no rendering. | `bevy_ecs`, `bevy_app`, `bevy_reflect` |
| `oddity_project` | `oddity.toml`: create, load, save, find, version pinning. | `serde`, `toml` |
| `oddity_icons` | Godot, @icons, brand and UI icons as embedded SVG, rendered at any scale; `UiIcon`. | `bevy`, `resvg` |
| `oddity_widgets` | The theme (Godot's palette rules + Oddity accent), fonts, and editor widgets: docks, splits, tree view, search fields, tool buttons, hover feedback. Reused by the Hub and plugins. | `bevy` (+ Feathers), `oddity_icons` |
| `oddity-editor` (app) | Puts it together: window chrome, docks, layout, shortcuts. | all of the above |

Rules of thumb:

- **Logic goes in a crate, wiring goes in the app.** If the Hub, the CLI or a
  plugin could need it, it doesn't belong in `apps/editor`.
- **`oddity_core` stays headless.** It must never depend on `bevy` (the full
  engine), rendering or UI, so it builds in seconds and runs in CLI/MCP/tests.
- **Bevy is pinned at `=0.20.0`.** It is our seed, not a moving dependency.

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
Revert. Both file types are BSN underneath.

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
└── docks/         one module per dock; mod.rs lists which tabs each dock has
```

Adding a dock page: write `docks/<name>.rs` with a `<name>_page()` scene and
(if it needs systems) a plugin, then add it as a tab in `docks/mod.rs`.

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
