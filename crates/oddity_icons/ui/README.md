# Oddity UI icons

Small icons Oddity draws itself, for things Godot's icon set does not cover
(Godot uses the operating system's title bar). Same rules as Godot's icons:
16×16 grid, neutral colour `#e0e0e0` so they can be tinted.

| File | Use |
| --- | --- |
| `window-minimize.svg` | Title bar: minimise |
| `window-maximize.svg` | Title bar: maximise |
| `window-restore.svg` | Title bar: restore (shown while maximised) |
| `window-close.svg` | Title bar: close |

In code: `Icon::ui("window-close")`.
