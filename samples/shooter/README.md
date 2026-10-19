# Shooter

The small top-down shooter from
[roadmap/scripting.md](../../roadmap/scripting.md#what-a-script-looks-like),
used to test Oddity while scripting is built.

Play it (from this folder; it's its own Cargo workspace, sharing the
engine's `target/` folder):

```sh
cargo run                                   # the main level
cargo run -- --oddity-play levels/main.level
cargo run -- --oddity-describe              # the scripts, as JSON
cargo test                                  # the level, played headless
```

WASD or the left stick moves, Space or the left mouse button fires, and
the goblins come for you. The score is in the window title.

Open it in the editor:

```sh
cargo editor -- samples/shooter             # from the repository root
```

The scripts in `src/` are the spec's examples, with what doesn't exist yet
left out: until physics (task F) a bullet hits by distance, and the HUD and
the shot sound return with task G. `src/main.rs` plays the part of the
`.oddity/runner` that new projects get from `Project::create_game`, so the
sample runs with a plain `cargo run`. The level and prefabs show
their shapes with `MeshShape`, until levels can refer to mesh assets.
