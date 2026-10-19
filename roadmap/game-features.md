# Game features

*Phase 4. What a game needs beyond scripting: assets, animation, UI,
saving, exporting, Steam, 2D, navigation, particles. This is the rough plan;
each feature gets a full spec, like [scripting.md](scripting.md), when its
turn comes. Crate versions were checked on 14 October 2026.*

The order below is the build order: each feature unlocks the next kind of game.

Every feature here follows the shape in [modularity.md](modularity.md): an
`oddity_<feature>` runtime crate, an `oddity_<feature>_editor` crate if it
has editor UI, and back-end crates where the engine underneath could change
(navigation on `landmass`, particles once a particle crate supports Bevy 0.20).
Features that pull in a third-party crate sit behind a Cargo feature on
`oddity`; features built only on Bevy are on by default.

| # | Feature | Unity / Godot | Built on | Ready for Bevy 0.20? |
| --- | --- | --- | --- | --- |
| 1 | Assets and importing | Import settings, `.meta` | Bevy assets, glTF | ✓ (Bevy) |
| 2 | Materials | Material assets | `StandardMaterial` | ✓ (Bevy) |
| 3 | Animation | Animator / AnimationPlayer | `bevy_animation` | ✓ (Bevy) |
| 4 | Game UI | uGUI / Control nodes | Bevy UI | ✓ (Bevy) |
| 5 | Moving between levels | `SceneManager`, autoloads | `oddity_level` | ✓ (ours) |
| 6 | Saving progress | `PlayerPrefs`, save files | `serde` | ✓ (ours) |
| 7 | Exporting games | Build Settings / Export | cargo, release profiles | ✓ (ours) |
| 8 | Steam | Steamworks.NET / GodotSteam | `steamworks` 0.13 | ✓ ([steam.md](steam.md)) |
| 9 | 2D | 2D mode | Bevy 2D, sprites | ✓ (Bevy); tilemaps ✗ |
| 10 | Navigation | NavMesh / NavigationAgent | `landmass` 0.9 | ✓ (engine-independent) |
| 11 | Particles | Particle System / GPUParticles | `bevy_hanabi` | ✗ (needs Bevy 0.19) |

## 1 · Assets and importing

- Every asset has an `.import` file with its ID and import settings (decided
  in [scripting.md](scripting.md#stable-ids-and-import-files)).
- **glTF models** import as a model asset you drop into a level, like Unity's
  model prefabs: meshes, materials, skeletons and animations come along.
- **Textures:** filtering, wrap mode, sRGB or linear, compression.
- **Audio:** streaming or loaded into memory, loop points.
- The **Import dock** (already in the editor) edits the selected asset's
  import settings. Reimport happens on change, and Bevy's `file_watcher` hot-
  reloads anything edited outside the editor.
- FileSystem thumbnails for models, textures and materials.

## 2 · Materials

- `.material` files (JSON) for `StandardMaterial`: colour, textures, metallic,
  roughness, emissive, transparency. Edited in the Inspector, with a preview
  sphere.
- Custom shaders (WGSL) come after.

## 3 · Animation

- An **AnimationPlayer** node plays clips from imported models;
  `ctx.play_animation("run")`, crossfades, and an `on_finished` event.
- An **Animation dock** with a timeline to make simple clips in the editor:
  move, rotate, change any Inspector field over time.
- Later: state machines and blend trees (Unity's Animator), on Bevy's
  `AnimationGraph`.

## 4 · Game UI

- The full set of Control nodes: containers (VBox, HBox, Grid, Margin,
  Scroll), Label, Button, TextureRect, ProgressBar, Slider, CheckBox,
  LineEdit, Panel.
- Anchors and layout edited in the 2D view, as Godot does.
- Game UI themes and fonts as assets (separate from the editor's own theme).
- Buttons and other controls expose events (`on_pressed`, `on_value_changed`)
  for the Inspector, like Unity's `onClick`.

## 5 · Moving between levels

- `ctx.load_level(..)` (from scripting) gets loading screens: load in the
  background, report progress, switch when ready.
- **Global nodes:** levels listed in Project Settings › Globals load once and
  survive level changes, like Godot's autoloads and Unity's
  `DontDestroyOnLoad`. Good for music, the score, the network manager.
- Additive loading: `ctx.load_level_additive(..)` and `ctx.unload_level(..)`.

## 6 · Saving progress

```rust
#[derive(Serialize, Deserialize, Default)]
struct SaveGame { level: String, score: u32, unlocked: Vec<String> }

ctx.save("slot1", &save)?;                       // to the player's save folder
let save: SaveGame = ctx.load("slot1")?.unwrap_or_default();
ctx.prefs().set("volume", 0.8);                  // small settings, like PlayerPrefs
```

- Saves go to the right folder per platform; with Steam enabled, they also
  sync through Steam Cloud.
- Saved data carries a version number, so a game update can upgrade old saves.

## 7 · Exporting games

- An **Export dialog**: Windows, Linux, macOS and Web; name, icon, version.
- Builds a release binary, copies or packs the assets, and writes everything
  to an output folder, ready to zip or upload.
- Options: include Steam ([steam.md](steam.md) S4), dedicated server
  ([multiplayer.md](multiplayer.md) N7).
- The Web export documents its limits (no threads, some audio restrictions).

## 8 · Steam

See [steam.md](steam.md): achievements, stats, rich presence, overlay,
lobbies, leaderboards, cloud saves, DLC checks.

## 9 · 2D

- The 2D view with Godot's rulers and canvas.
- `Node2D`, `Sprite2D`, `AnimatedSprite2D`, `Camera2D`, and 2D physics nodes
  (`RigidBody2D`, `StaticBody2D`, `CharacterBody2D`, `Area2D`,
  `CollisionShape2D`) on `bevy_rapier2d` behind the same physics layer.
- Tilemaps: `bevy_ecs_tilemap` needs Bevy 0.19 today. Wait for it, or write a
  small tilemap node of our own on Bevy's sprites.

## 10 · Navigation

- **NavigationRegion3D** bakes a navigation mesh from the level's static
  geometry; **NavigationAgent3D** walks it: `agent.set_target(..)`, steering
  around other agents.
- Built on **`landmass`** 0.9, which doesn't depend on Bevy (only on the
  `glam` maths library), with our own small Bevy wrapper. The usual Bevy
  wrappers (`bevy_landmass`, `vleue_navigator`) need Bevy 0.19 today.
  Navmesh baking still needs a solution; that's the first question for this
  feature's spec.

## 11 · Particles

- A **ParticleEmitter3D** node: rate, lifetime, velocity, size and colour over
  life, textures.
- `bevy_hanabi` (GPU particles) needs Bevy 0.19 today. The plan is a simple
  CPU emitter of our own first, enough for sparks, smoke and hits, then GPU
  particles when Hanabi supports 0.20 or as our own port.

## Later

- An audio mixer with buses and effects.
- Localisation: string tables, per-language fonts.
- Modding: loading user content, building on prefabs and plugin support.
- Consoles: needs each platform's SDK and licence; Rust support varies.
- VR / XR.
