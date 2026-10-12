# @icons

The SVG files in `icons/` are from **@icons** by Valentin Fossati (Voxybuns)
and contributors: <https://github.com/Voxybuns/at-icons>
(commit `bc002842ea6ed19e10f642d159af8e92c93a1597`, v1.5.0).

They are MIT-licensed; see `LICENSE.txt` and `AUTHORS.md`.

Only the neutral `node/` colour set is copied. @icons ships the same icons in
six colours (node, node2d, node3d, control, mesh, animation); Oddity recolours
the neutral set at runtime instead (`Icon::at("hammer").tinted(NodeColor::Node3D)`),
the same way Godot recolours its own icons.
