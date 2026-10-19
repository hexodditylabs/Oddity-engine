# Scripting

*Decided 14 October 2026. This is the design and the build plan for game
code in Oddity. Read it before working on any scripting task; the tasks at
the end are meant to be done one at a time, in order.*

**Contents:** [Decisions](#decisions) ·
[What a script looks like](#what-a-script-looks-like) ·
[Reference](#reference) · [How it works](#how-it-works) ·
[Crates](#crates) · [Build plan](#build-plan) · [Later](#later)

---

## Decisions

| Question | Decision |
| --- | --- |
| Style | **Unity-style scripts in Rust.** A script is a struct; a node can have several. |
| Callback names | `_onready`, `_onprocess`, `_onphysicsprocess`, `_onlateprocess`, `_onenable`, `_ondisable`, `_ondestroy`, `_oncollisionenter`/`exit`, `_ontriggerenter`/`exit` |
| Inspector fields | `pub` fields show and are saved; `#[show]` shows a private field; `#[hide]` hides a `pub` one. Private fields are runtime-only. |
| When changes apply | **Immediately**, as in Unity. Scripts run one at a time on the main thread. |
| Order | Hierarchy order, top to bottom; `#[script(order = n)]` moves a script type earlier or later. |
| Other logic | Plain Bevy systems, alongside scripts, in parallel as usual. Scripts are components, so systems can query them. |
| Events | Inspector-wired `Event` fields (Unity's `UnityEvent`), the same wired from code, and game-wide `#[signal]`s. |
| The editor and your code | **The editor never contains or runs your code.** It holds script values as data, learns your types from your built game, and updates the Inspector without restarting. |
| Building | Your game crate compiles in the background when a file changes (Unity-style). The editor and Bevy are prebuilt or cached, never rebuilt per project. |
| Play | Your game runs as **its own process**, with a **live link**: the Hierarchy and Inspector show the running game and can tweak it. Stop discards those changes, as in Unity. |
| Physics | Our own physics nodes and callbacks. **Rapier** (`bevy_rapier3d` 0.37, which supports Bevy 0.20) underneath. Game code never names Rapier, so the engine can be swapped later (e.g. to Avian once it supports 0.20). |
| Files | Levels and prefabs are **JSON** (see [the level format](#the-level-format) for why not BSN or RON). |
| Renaming | Scripts and assets have **stable IDs** (`#[script(id = "...")]`, `.import` files next to assets, like Unity's `.meta` and Godot's `.import`), so moving or renaming never breaks a level. |
| Errors | A script that panics is **switched off and reported**, with its node and file:line; the game keeps running, as in Unity. |
| Pause | `ctx.pause`, a time scale, and a per-node **process mode** (Godot's), so a pause menu keeps working while the game is paused. |
| Inspector hints | Unity's field attributes: `#[range]`, `#[min]`, `#[tooltip]`, `#[header]`, `#[multiline]`. |
| Testing | Game code is testable with `cargo test`: load a level headless, step frames, check results. |
| 2D / 3D | **3D first**, 2D right after. |
| No built-in code editor | Scripts open in VS Code or Zed. |

## What a script looks like

A small top-down shooter. The level:

```
Main
├── Camera           [CameraFollow]
├── Player           [Player] [Health]          CharacterBody3D
│   ├── Shape                                    CollisionShape3D
│   └── Muzzle
├── Enemies
│   ├── Goblin       [Enemy] [Health]            instance of goblin.prefab
│   └── Goblin2      [Enemy] [Health]            instance of goblin.prefab
└── HUD
    └── EnemyCount   [EnemyCountLabel]           Label
```

`src/health.rs`, one script reused by the player and every enemy:

```rust
use oddity::prelude::*;

#[script]
pub struct Health {
    #[default(100.0)]
    pub max: f32,
    /// Fired once when health reaches zero. Listeners are added in the Inspector.
    pub on_died: Event,
    /// Fired on every hit, with the damage taken.
    pub on_damaged: Event<f32>,
    current: f32,
}

impl Script for Health {
    fn _onready(&mut self, _ctx: &mut Ctx) {
        self.current = self.max;
    }
}

impl Health {
    pub fn damage(&mut self, amount: f32, ctx: &mut Ctx) {
        if self.current <= 0.0 {
            return;
        }
        self.current -= amount;
        self.on_damaged.invoke(ctx, amount);
        if self.current <= 0.0 {
            self.on_died.invoke(ctx);
        }
    }
}
```

`src/player.rs`:

```rust
use oddity::prelude::*;

#[script]
pub struct Player {
    #[default(6.0)]
    pub speed: f32,
    #[default(0.25)]
    pub fire_rate: f32,          // seconds between shots
    pub bullet: Prefab,          // dragged in from the FileSystem
    pub muzzle: NodeRef,         // dragged in from the Hierarchy
    pub shoot_sound: AudioClip,
    cooldown: f32,               // private: runtime only
}

impl Script for Player {
    fn _onprocess(&mut self, ctx: &mut Ctx, delta: f32) {
        let movement = ctx.input().axis2("move");               // WASD or left stick
        let step = Vec3::new(movement.x, 0.0, -movement.y) * self.speed * delta;
        ctx.translate(step);

        self.cooldown -= delta;
        if ctx.input().action_pressed("fire") && self.cooldown <= 0.0 {
            self.cooldown = self.fire_rate;
            let at = ctx
                .global_transform_of(self.muzzle)
                .unwrap_or(ctx.global_transform());
            ctx.instantiate(&self.bullet, at);
            ctx.play_sound(&self.shoot_sound);
        }
    }
}
```

`src/bullet.rs`, a RigidBody3D with a CollisionShape3D child:

```rust
use oddity::prelude::*;

#[script]
pub struct Bullet {
    #[default(20.0)]
    pub speed: f32,
    #[default(10.0)]
    pub damage: f32,
}

impl Script for Bullet {
    fn _onready(&mut self, ctx: &mut Ctx) {
        ctx.after(2.0, "expire");                          // Unity's Invoke
    }

    fn _onprocess(&mut self, ctx: &mut Ctx, delta: f32) {
        let forward = ctx.forward();
        ctx.translate(forward * self.speed * delta);
    }

    fn _oncollisionenter(&mut self, ctx: &mut Ctx, hit: &Collision) {
        let damage = self.damage;
        ctx.call::<Health>(hit.other, move |health, ctx| health.damage(damage, ctx));
        ctx.destroy();
    }
}

#[script]
impl Bullet {
    #[listener]
    fn expire(&mut self, ctx: &mut Ctx) {
        ctx.destroy();
    }
}
```

`src/enemy.rs`:

```rust
use oddity::prelude::*;

#[signal]
pub struct EnemyKilled {
    pub score: u32,
}

#[script]
pub struct Enemy {
    #[default(3.0)]
    pub speed: f32,
    #[default(20.0)]
    pub damage_per_second: f32,
    #[default(10)]
    pub score: u32,
    pub target: NodeRef,
}

impl Script for Enemy {
    fn _onready(&mut self, ctx: &mut Ctx) {
        // Wired from code here; it could equally be wired in the Inspector.
        let me = ctx.node();
        ctx.connect(me, "on_died", "die");
        if self.target.is_none() {
            self.target = ctx.all::<Player>().first().copied().unwrap_or_default();
        }
    }

    fn _onprocess(&mut self, ctx: &mut Ctx, delta: f32) {
        let Some(target) = ctx.position_of(self.target) else { return };
        let to_target = target - ctx.position();
        if to_target.length() > 1.0 {
            ctx.translate(to_target.normalize() * self.speed * delta);
            ctx.look_at(target);
        } else {
            let damage = self.damage_per_second * delta;
            ctx.call::<Health>(self.target, move |health, ctx| health.damage(damage, ctx));
        }
    }
}

#[script]
impl Enemy {
    #[listener]
    fn die(&mut self, ctx: &mut Ctx) {
        ctx.emit(EnemyKilled { score: self.score });
        ctx.destroy();
    }
}
```

`src/camera_follow.rs`:

```rust
use oddity::prelude::*;

#[script]
pub struct CameraFollow {
    pub target: NodeRef,
    #[default(Vec3::new(0.0, 10.0, 8.0))]
    pub offset: Vec3,
    #[default(5.0)]
    pub smoothing: f32,
}

impl Script for CameraFollow {
    // Late, so it follows where everything ended up this frame.
    fn _onlateprocess(&mut self, ctx: &mut Ctx, delta: f32) {
        let Some(target) = ctx.position_of(self.target) else { return };
        let t = 1.0 - (-self.smoothing * delta).exp();
        let position = ctx.position().lerp(target + self.offset, t);
        ctx.set_position(position);
        ctx.look_at(target);
    }
}
```

`src/lib.rs`, with plain Bevy systems next to the scripts:

```rust
use oddity::prelude::*;

mod bullet;
mod camera_follow;
mod enemy;
mod health;
mod player;

/// Tags the HUD label. A script with no fields works as a tag.
#[script]
pub struct EnemyCountLabel;

/// Game-wide score: a plain Bevy resource (Unity's singleton, Godot's autoload).
#[derive(Resource, Default)]
pub struct Score(pub u32);

/// Everything the game adds besides scripts, which register themselves.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Score>()
            .add_systems(Update, (count_score, show_enemy_count));
    }
}

fn count_score(mut kills: MessageReader<enemy::EnemyKilled>, mut score: ResMut<Score>) {
    for kill in kills.read() {
        score.0 += kill.score;
    }
}

fn show_enemy_count(
    enemies: Query<(), With<enemy::Enemy>>,
    mut label: Single<&mut Text, With<EnemyCountLabel>>,
) {
    label.0 = format!("Enemies: {}", enemies.iter().count());
}
```

The Inspector for the Player node:

```
Player                                   CharacterBody3D
▸ Transform
▾ ☑ Player                                         script
    Speed          6
    Fire Rate      0.25
    Bullet         [ bullet.prefab ]
    Muzzle         [ Muzzle ]
    Shoot Sound    [ shoot.ogg ]
▾ ☑ Health                                         script
    Max            100
    On Died        GameOverScreen › show         [+]
    On Damaged     HealthBar › set_damage        [+]
[ Add Component ]
```

The ☑ is Unity's per-script enable checkbox.

## Reference

The user-facing API. Names here are the contract; tasks implement them.

### `#[script]`

```rust
#[script(id = "4f9d2c71")]                  // the stable ID; New Script writes it
#[script(id = "4f9d2c71", order = -10)]     // runs before order-0 scripts each stage
pub struct Name { ... }
```

The `id` is what levels store, so you can rename the struct or move the file
freely. New Script writes a fresh one. A script without an `id` still works
(levels then store its Rust path), and the editor offers to add one.

`#[script]` makes the struct a Bevy `Component` with `Reflect`, registers it
automatically (no list to maintain), and implements `Default` from the field
defaults. Every script type must also `impl Script for Name {}`, even if empty,
so the runner can call it.

| Field | Inspector | Saved in the level |
| --- | --- | --- |
| `pub x: T` | shown | yes |
| `#[show] x: T` | shown | yes |
| `#[hide] pub x: T` | hidden | no |
| `x: T` | hidden | no, starts from its default every load |
| `#[default(expr)]` | the starting value; without it, `T::default()` | |

Field types the Inspector can edit: `bool`, all integers, `f32`, `f64`,
`String`, `Vec2`, `Vec3`, `Vec4`, `Quat` (shown as degrees), `Color`, enums,
structs and `Vec<T>` of these, plus `NodeRef`, `Prefab`, `AudioClip`, `Event`
and `Event<T>`.

### Inspector attributes

| Attribute | Unity | Effect |
| --- | --- | --- |
| `#[range(0.0, 10.0)]` | `[Range]` | A slider; values are clamped |
| `#[min(0)]` | `[Min]` | Never below this |
| `#[tooltip("...")]` | `[Tooltip]` | Shown on hover; `///` doc comments do the same |
| `#[header("Movement")]` | `[Header]` | A bold heading above the field |
| `#[multiline]` | `[TextArea]` | A multi-line text box for a `String` |

### Callbacks

All optional. `delta` is in seconds.

| Callback | Unity | When |
| --- | --- | --- |
| `_onready(&mut self, ctx)` | `Start` | Once, before the node's first `_onprocess` |
| `_onprocess(&mut self, ctx, delta)` | `Update` | Every frame |
| `_onphysicsprocess(&mut self, ctx, delta)` | `FixedUpdate` | At the fixed physics rate |
| `_onlateprocess(&mut self, ctx, delta)` | `LateUpdate` | Every frame, after all `_onprocess` |
| `_onenable(&mut self, ctx)` | `OnEnable` | Before `_onready`, and whenever the script is switched back on |
| `_ondisable(&mut self, ctx)` | `OnDisable` | When switched off, and before `_ondestroy` |
| `_ondestroy(&mut self, ctx)` | `OnDestroy` | Just before the node is removed |
| `_oncollisionenter(&mut self, ctx, hit: &Collision)` | `OnCollisionEnter` | A physics body started touching this one |
| `_oncollisionexit(&mut self, ctx, hit: &Collision)` | `OnCollisionExit` | It stopped touching |
| `_ontriggerenter(&mut self, ctx, other: NodeRef)` | `OnTriggerEnter` | Something entered this Area3D |
| `_ontriggerexit(&mut self, ctx, other: NodeRef)` | `OnTriggerExit` | It left |

### `Ctx`

Everything a script does to the game goes through `ctx`.

**This node**

| | |
| --- | --- |
| `node() -> NodeRef` | This node |
| `name() -> &str` | Its name |
| `position()`, `set_position(Vec3)`, `translate(Vec3)` | Local position |
| `rotation()`, `set_rotation(Quat)`, `look_at(Vec3)`, `forward()` | Rotation |
| `transform()`, `transform_mut()`, `global_transform()` | The full transform |
| `get::<T>() -> Option<&T>`, `get_mut::<T>() -> Option<Mut<T>>` | Unity's `GetComponent`, on this node |
| `enable::<S>(bool)` | Switch a script on this node on or off |

**Other nodes**

| | |
| --- | --- |
| `has::<T>(node)`, `get_on::<T>(node)`, `get_mut_on::<T>(node)` | Components on another node |
| `position_of(node)`, `global_transform_of(node)` | `Option`, `None` if the node is gone |
| `call::<S>(node, move \|script, ctx\| ...)` | Run code on another node's script, with that node's own `ctx`. A call to a script that is running higher up the stack waits until it finishes, which is why the closure is `move` |
| `parent()`, `children()`, `find("Enemies/Goblin")`, `find_child("Muzzle")` | Finding by hierarchy |
| `all::<S>() -> Vec<NodeRef>` | Every node with that script (`FindObjectsOfType`) |
| `in_group("enemies") -> Vec<NodeRef>`, `add_to_group("enemies")`, `remove_from_group("enemies")` | Groups (Unity tags, Godot groups): the group's nodes in Hierarchy order; adding and removing act on this node |

**Creating and removing**

| | |
| --- | --- |
| `instantiate(&Prefab, Transform) -> NodeRef` | Spawn a prefab in this node's level (a child of its root); a `GlobalTransform` works too |
| `destroy()`, `destroy_node(node)` | Remove, running `_ondisable` and `_ondestroy` |
| `load_level("levels/cave.level")` | Replace the current level |

**Events, signals, timers**

| | |
| --- | --- |
| `event.invoke(ctx)` / `event.invoke(ctx, value)` | Call every listener now |
| `connect(source, "event_field", "listener")` | Listen to the `Event` field of that name on `source` (whichever of its scripts has it) with this script's listener; `disconnect(..)` undoes it |
| `emit(signal)` | Send a game-wide `#[signal]` |
| `after(seconds, "listener")`, `every(seconds, "listener")` | Timers; `cancel_timers()` stops this node's |

**Everything else**

| | |
| --- | --- |
| `input()` | `pressed`/`just_pressed`/`just_released(KeyCode)`; `mouse_pressed`/`mouse_just_pressed`/`mouse_just_released(MouseButton)`, `mouse_motion()`, `mouse_scroll()`, `cursor_position()`; `action_pressed(name)`, `action_just_pressed(name)`, `action_just_released(name)`, `action_strength(name)`, `axis(neg, pos)`, `axis2(action)`. An action or axis missing from the input map panics, which switches the script off with its name |
| `delta()`, `time()` | Frame time, time since start |
| `resource::<R>()`, `resource_mut::<R>()` | Bevy resources |
| `raycast(origin, direction, max) -> Option<RayHit>` | Physics query |
| `play_sound(&AudioClip)` | Audio |
| `draw_line(a, b, Color)`, `draw_sphere(at, radius, Color)` | Debug drawing for one frame |
| `world_mut() -> &mut World` | Escape hatch to plain Bevy |

### Node references, prefabs, assets

- `NodeRef` is a `Copy` handle to a node; `NodeRef::NONE` is empty and
  `is_none()` checks for it. In the editor you drag a node from the Hierarchy
  onto the field. It survives saving, loading and prefab instancing.
- `Prefab` and `AudioClip` are asset references; drag a file from the
  FileSystem onto the field.

### Events and listeners

```rust
pub on_died: Event,             // no data
pub on_damaged: Event<f32>,     // carries a value
```

A listener is a method marked `#[listener]`, in an impl block marked
`#[script]` (which builds the type's table of listeners), taking `(&mut self, &mut Ctx)` or
`(&mut self, &mut Ctx, value: &T)`. The Inspector lists listeners by name; so
do `connect`, `after` and `every`.

### Signals

```rust
#[signal]
pub struct EnemyKilled { pub score: u32 }

ctx.emit(EnemyKilled { score: 10 });

#[on(EnemyKilled)]                 // in the script's `#[script] impl` block, like listeners
fn add_score(&mut self, ctx: &mut Ctx, killed: &EnemyKilled) { .. }

fn system(mut kills: MessageReader<EnemyKilled>) { .. }   // systems read them too
```

`#[on]` handlers run right after the script that emitted finishes, on every
node that has a handler, in Hierarchy order; switched-off scripts miss it.
A signal is cloned for each reader, so `#[signal]` derives `Clone` and its
fields must be `Clone` and reflectable.

### Physics

| Node | What it is |
| --- | --- |
| `RigidBody3D` | Moved by physics (mass, gravity scale, damping) |
| `StaticBody3D` | Never moves: floors, walls |
| `CharacterBody3D` | Moved by your code, collides with the world |
| `Area3D` | A trigger: detects overlaps, doesn't block |
| `CollisionShape3D` | Child of any of the above: box, sphere, capsule, cylinder |

`Collision` has `other: NodeRef`, `point: Vec3`, `normal: Vec3`. `RayHit`
has `node`, `point`, `normal`, `distance`.

### Input actions

Defined in Project Settings › Input Map and saved in `oddity.toml`:

```toml
[input.actions]
fire = ["Space", "MouseLeft", "GamepadSouth"]
jump = ["KeyW"]

[input.axes2]
move = { left = "KeyA", right = "KeyD", up = "KeyW", down = "KeyS", stick = "LeftStick" }
```

A binding is a Bevy `KeyCode` name (`"Space"`, `"KeyW"`), `"Mouse"` plus a
`MouseButton` (`"MouseLeft"`), or `"Gamepad"` plus a `GamepadButton`
(`"GamepadSouth"`); a stick is `"LeftStick"`, `"RightStick"` or `"DPad"`.
Gamepad bindings count on every connected gamepad. `axis2` has +y up and
is at most 1 long.

### Built-in nodes

A node type is a preset of Bevy components, with a Godot icon. Scripts use
those Bevy components directly, so everything in Bevy's docs applies.

| Node | Made of | From a script |
| --- | --- | --- |
| `Node` | `Name` | |
| `Node3D` | `Transform`, `Visibility` | `ctx.position()` etc. |
| `MeshInstance3D` | `Mesh3d`, `MeshMaterial3d`; until levels can refer to mesh assets (Phase 4), a `MeshShape` (box, sphere, capsule, cylinder or plane, and a colour) saved inline, as Godot saves a `BoxMesh` in a scene | `ctx.get_mut::<MeshMaterial3d<StandardMaterial>>()` |
| `Camera3D` | `Camera3d`, `Camera`, `Projection` | `ctx.get_mut::<Projection>()` for the field of view |
| `DirectionalLight3D`, `OmniLight3D`, `SpotLight3D` | `DirectionalLight`, `PointLight`, `SpotLight` | `ctx.get_mut::<PointLight>().intensity` |
| `WorldEnvironment` | sky, ambient light, fog settings | |
| `AudioPlayer3D` | `AudioPlayer`, `PlaybackSettings`, `SpatialAudio` | |
| Physics nodes | see [Physics](#physics) | |
| `Control`, `Label`, `Button`, `ProgressBar`, `TextureRect`, `Panel`, `VBox`, `HBox` | Bevy UI (`Node`, `Text`, `ImageNode`, ...) | `ctx.get_mut::<Text>()` |

The catalogue grows with each feature (animation, particles, navigation); it
lives in the node registry, which also feeds the Add Node dialog.

### Pause and time

| | |
| --- | --- |
| `ctx.pause(true)`, `ctx.is_paused()` | Pause the game |
| `ctx.set_time_scale(0.5)`, `ctx.time_scale()` | Slow motion from the next frame; `delta`, timers and physics are scaled |
| Process mode (Inspector, per node) | **Inherit** (default), **Pausable**, **When paused**, **Always**, **Disabled**, as in Godot |

A pause menu's root node uses *When paused* or *Always*, so its scripts keep
running while everything *Pausable* stops. Physics stops while paused.

### Errors, logging and debugging

- **A panicking script doesn't stop the game.** Output shows the panic message,
  the node, the script and the file:line, clickable. That script is switched
  off on that node; everything else keeps running. Release builds behave the
  same, unless the game sets `panic = "abort"`.
- **Logging:** `info!`, `warn!` and `error!` from scripts appear in Output
  tagged with the node that logged them; click to select it. The runner
  wraps every script call in a `script` span with the node's name, its
  entity and the script's type path
  (`script{node=Goblin entity=12v0 script=my_game::Enemy}: ...`); a script
  called from another logs inside its caller's span, under its own.
- **Breakpoints:** attach VS Code's debugger (CodeLLDB) or Zed's to the running
  game program; the project template includes the launch settings.

### Testing your game code

```rust
#[test]
fn goblins_die_when_shot() {
    let mut game = oddity::test::Game::load("levels/arena.level");
    let goblin = game.find("Enemies/Goblin");
    game.call::<Health>(goblin, |health, ctx| health.damage(1000.0, ctx));
    game.step(1);
    assert!(!game.exists(goblin));
}
```

`oddity::test::Game` runs the game headless (no window, no GPU), so these run
in `cargo test` and in CI. Every frame lasts 1/64 s, Bevy's fixed step, so a
test gives the same result on every machine. Starting a level runs its first
frame: every script gets `_onready`, then `_onprocess` with a `delta` of 0.

`Game::load(path)` reads the level file relative to the crate being tested.
A test can also build its level in code, and adds the game's own systems with
`add_plugins` (`with_level_file(path)` loads a file after them):

```rust
let mut game = Game::new().add_plugins(GamePlugin).with_level(|level| {
    level
        .spawn(Name::new("Enemies"))
        .with_child((Name::new("Goblin"), Health::default(), Enemy::default()));
});
```

`Game` also has `get::<T>(node)`, `level()`, `world()` and `world_mut()`.

## How it works

![How your code reaches the game](images/scripting-architecture.svg)

### Project layout

```
my_game/
├── oddity.toml
├── Cargo.toml            the game crate: a library, depends on `oddity`
├── src/                  scripts and systems; `pub struct GamePlugin` in lib.rs
├── levels/  prefabs/  assets/
└── .oddity/              generated by the editor, git-ignored
    ├── runner/           a 3-line binary crate: oddity::run::<my_game::GamePlugin>()
    ├── types.json        what the last build said about your types
    └── play/             the level being played, saved there with unsaved edits
```

`oddity` is the one crate games depend on: the prelude, the macros and the
runtime. Until it's published, the template points it at the engine's git
repository, at the tag of the editor's version.

All projects on a machine share one Cargo target folder per editor version, so
Bevy compiles once per machine, not once per project: `oddity/target/<version>`
in the OS's cache folder (`%LOCALAPPDATA%` on Windows, `~/.cache` on Linux,
`~/Library/Caches` on macOS), set in the project's `.cargo/config.toml`, which
also adds `cargo play`. A new project also gets a main level (a camera, a
light and a floor) and a `move` input axis.

### The editor never runs your code

In the editor, a script on a node is data: `ScriptInstance { type_path,
values }`, where `values` are its saved fields as JSON. The Inspector draws
them from the type description; edits are ordinary undoable commands. Built-in
components (Transform, Visibility, physics nodes) are real types in the editor,
as today.

A component the editor can't place at all (its script was deleted, say) is
kept as-is and shown greyed out with a warning, so opening and saving a level
never loses data.

### Build and describe

1. A file under `src/` changes; the editor waits half a second for more changes.
2. It runs `cargo build --message-format=json-diagnostic-rendered-ansi` on
   `.oddity/runner`. Compiler messages stream to Output; errors are clickable.
3. On success it runs the runner with `--oddity-describe`. The game prints its
   type description and exits.
4. The editor reads it into `.oddity/types.json`, then refreshes every
   Inspector: new fields get their defaults, removed fields are flagged.

`--oddity-describe` prints:

```json
{
  "scripts": [{
    "type_path": "my_game::player::Player",
    "name": "Player",
    "order": 0,
    "fields": [
      { "name": "speed", "type": "f32", "default": 6.0 },
      { "name": "bullet", "type": "Prefab", "default": null },
      { "name": "muzzle", "type": "NodeRef", "default": null }
    ],
    "listeners": [{ "name": "die", "arg": null }],
    "events": [{ "field": "on_died", "arg": null }]
  }],
  "components": [],
  "signals": [{ "type_path": "my_game::enemy::EnemyKilled" }],
  "types": { "...": "nested structs and enums used by fields" }
}
```

### The level format

`.level` and `.prefab` files are pretty-printed JSON:

```json
{
  "format": "oddity-level",
  "version": 1,
  "nodes": [
    { "id": 1, "name": "Main", "parent": null, "kind": "Node3D",
      "components": {
        "bevy_transform::components::transform::Transform": { "translation": [0, 0, 0], "...": "..." }
      } },
    { "id": 3, "name": "Player", "parent": 1, "kind": "CharacterBody3D",
      "components": {
        "script:4f9d2c71": {
          "speed": 6.0,
          "bullet": { "asset": "a3b1c0de-…", "path": "prefabs/bullet.prefab" },
          "muzzle": 4
        }
      } },
    { "id": 7, "name": "Goblin", "parent": 5,
      "prefab": { "asset": "77e0f2aa-…", "path": "prefabs/goblin.prefab" },
      "overrides": [
        { "node": ".", "component": "script:9c2e5b10", "field": "max", "value": 30.0 }
      ] }
  ]
}
```

- Nodes are listed parents first, siblings in order; `id`s are only meaningful
  inside the file, and exactly one node (the level's root) has no parent. A
  node keeps its `id` from save to save, and new nodes get the next free
  number, so references stay right in data kept as written, and diffs stay
  small. A
  `NodeRef`, and any plain `Entity` field, is saved as the target's `id`, or
  `null` when the target isn't in the level. Deleted (`Disabled`) nodes
  aren't saved.
- What Bevy works out by itself (`GlobalTransform`, `InheritedVisibility`,
  `ViewVisibility`, ...) isn't saved; it comes back from the components it
  depends on. Nor is a component another one on the node requires (a camera
  brings a dozen) while it still has its default value: loading brings it
  back. Loading inserts a node's requiring components first, so what they
  bring in is what they want, then the saved values over it.
- A prefab instance stores the prefab and its overrides only. `node` is a
  path inside the prefab (`.` is the prefab's root); `field` is a reflect
  path (`max`, `translation.x`), or left out to set the whole component,
  which also adds one the prefab lacks. A `NodeRef` in an override's
  `value` is a level `id`. The prefab's own nodes aren't saved in the level;
  nodes added below an instance's root are, as level nodes. If the prefab
  can't be read, the instance loads as an empty node that keeps its prefab
  and overrides, so saving loses nothing.
- `.prefab` files have the same shape, with `"format": "oddity-prefab"`;
  the root is the first node. A reference inside a prefab points at the same
  instance's node. Prefabs inside prefabs come later.
- Scripts are stored as `script:<id>` (or by their Rust path without an
  `id`), with their saved fields only; built-in components by their Rust
  path. Loading starts a script from its defaults and reads the saved fields
  over them; a field the script no longer has is dropped with a warning.
- A component that can't be read (its type is unknown, as for scripts in the
  editor, or it no longer parses) is kept as written and saved again
  unchanged; so are a node's keys a newer version added.
- Assets are stored by ID; `path` is only a hint for people reading the file
  and for recovering a reference whose `.import` file was lost.

**Why JSON, not BSN or RON.** Bevy 0.20 has BSN only as the `bsn!` macro; its
docs say the `.bsn` file format "is not yet released". The editor holds game
values without the game's types, and JSON round-trips them losslessly through
reflection. RON needs the real types to write some values (tuple structs)
correctly. The game reads the files with its real types, through our loader
in `oddity_level`.

### Stable IDs and `.import` files

Every asset gets an `.import` file next to it, as in Godot (Unity calls its
`.meta`):

```
prefabs/bullet.prefab
prefabs/bullet.prefab.import   { "id": "a3b1c0de-…", "import": { … } }
```

The `.import` file holds the asset's ID (a random UUID) and its import
settings (what the Import dock edits); keys it doesn't know are kept. The
editor creates `.import` files for new files, and moves them along when you
rename or move things in the FileSystem dock. Moving files outside the editor
works if the `.import` file moves too; otherwise the editor lists the broken
references and offers to re-link them by path. A file that lost its
`.import` file gets its old ID back from the first reference that names
its path. A copy made outside the editor, `.import` file and all, gets a new
ID and keeps the copied settings.

**Why not `.meta`.** Bevy reads `<asset>.meta` as its own loader settings
(`AssetMetaCheck::Always`, the default) and fails the load when the file is
anything else, so ours would stop Bevy loading the asset beside it. A name of
our own leaves Bevy's `.meta` and its asset processor free.

Assets are the files in the project's folders, except `src/`, `target/` and
anything whose name starts with a dot; files directly in the project folder
(`oddity.toml`, `Cargo.toml`) aren't assets. `oddity_project::AssetIndex`
maps IDs to paths both ways, and creates, renames and moves assets with their
`.import` files.

Scripts carry their ID in `#[script(id = "...")]` instead, because one Rust file
can hold several scripts.

### The runner

One exclusive system per stage (`Update`, `FixedUpdate`, `PostUpdate` before
transform propagation) runs scripts one at a time:

1. Walk the level in Hierarchy order (cached; rebuilt when the hierarchy
   changes), and within a node, scripts by `order`, then the order they were added.
2. For each script: **take it out** of its component (`std::mem::take`,
   leaving the default in place; no archetype change), build a `Ctx` holding
   `&mut World` and the entity, call the callback, then **put it back**, if the
   node still exists. Taking and putting back bypass change detection.
3. Because the running script is out of its component, a script reaches its
   own data through `self`. If code reaches a script that is currently running
   higher up the call stack (a listener on itself, say), the call is **queued**
   and runs as soon as that script finishes. This is the one exception to
   "immediately".
4. Callbacks a script type doesn't implement are skipped. The trait's default
   bodies record "not implemented" the first time they're called, and the
   runner stops calling them for that type.
5. `_onready` runs before a script's first `_onprocess`, including for nodes
   spawned mid-frame. `_onenable` runs just before it.
6. A switched-off script is listed in the node's `DisabledScripts` and skipped.
7. `ctx.destroy()` runs `_ondisable` and `_ondestroy`, then despawns at the end
   of the current script. A system that despawns directly skips the callbacks,
   so systems use `oddity::destroy(&mut commands, entity)` instead. Likewise
   `oddity::call::<S>(world, node, f)` does from outside a script what
   `ctx.call` does.
8. Every callback runs inside `std::panic::catch_unwind`. On a panic, the
   runner puts the script back, adds it to `DisabledScripts`, drops anything it
   queued, and reports the panic with the node and script. This needs
   `panic = "unwind"`, Rust's default; the project template doesn't change it.
9. While paused, the runner skips nodes whose process mode resolves to
   *Pausable*, and runs those that resolve to *When paused* or *Always*. The
   time scale multiplies `delta` (Bevy's `Time<Virtual>`). Pausing is
   pausing `Time<Virtual>`, so game time, the fixed step, physics and
   pausable nodes' timers stand still; the scripts that still run get the
   real frame time as `delta`. Unlike Godot, `_onphysicsprocess` doesn't run
   at all while paused, on any node: Bevy's fixed step is driven by game
   time. `_onready` runs whatever the mode (as in Godot); events, signals
   and `call` aren't affected by pausing.

### Events, signals and timers underneath

- `#[script]` on an impl block turns its `#[listener]` methods into a table
  per script type: name → function. `Event`
  fields hold `(NodeRef, listener name)` pairs; `invoke` looks each one up and
  calls it through the runner's take-out/put-back.
- `#[signal]` derives `Message` (and registers itself, so `ScriptPlugin`
  adds its channel); `emit` writes the message (for systems) and
  records it for `#[on]` handlers, which the runner calls after the emitting
  script.
- Timers live in a `ScriptTimers` component per node; the runner ticks them in
  `Update` and calls the listener when one fires.

### Physics underneath

`oddity_physics` defines the node types, `Collision`, `RayHit` and the
callbacks; it doesn't know any physics engine. A backend plugin translates our
nodes into the engine's components, turns the engine's contact events into
ours, and answers raycasts. `oddity_physics_rapier` is the first backend.
Switching engines means writing another backend plugin; game code doesn't
change.

### Play and the live link

1. Play saves the open level with unsaved edits to `.oddity/play/`, builds if
   needed, and starts the runner with
   `--oddity-play .oddity/play/main.level --oddity-link 127.0.0.1:<port>`.
2. The runner adds Bevy's remote protocol (`bevy_remote`) when it gets
   `--oddity-link`. The editor polls it for the Hierarchy and the selected
   node's components, and sends Inspector edits back as component updates.
3. Output shows the game's log. A panic shows its message and location; the
   editor stays up.
4. Stop closes the game. Nothing it changed was ever written to the level, so
   there's nothing to revert.

## Crates

Layers and the rules between them are in [modularity.md](modularity.md#layers).

| Crate | Layer | What | Depends on |
| --- | --- | --- | --- |
| `oddity` | runtime, facade | What games depend on: prelude, re-exports, `oddity::run`; optional features behind Cargo features | everything else in runtime |
| `oddity_script` | runtime | `Script`, `Ctx`, the runner, `NodeRef`, `Event`, signals, timers, input map | `bevy` |
| `oddity_script_macros` | runtime | `#[script]`, `#[listener]`, `#[signal]`, `#[on]` | `syn`, `quote` |
| `oddity_level` | runtime | Level and prefab files: write, read, spawn, overrides | `bevy`, `oddity_script`, `oddity_project`, `serde_json`, `uuid` |
| `oddity_physics` | runtime | Physics nodes, `Collision`, `RayHit`, the backend interface | `bevy` |
| `oddity_physics_rapier` | runtime, backend | The Rapier backend | `oddity_physics`, `bevy_rapier3d` 0.37 |
| `oddity_physics_editor` | extension | Physics nodes in Add Node, collision shapes drawn in the viewport | `oddity_physics`, `oddity_plugin_api` |
| `oddity_build` | tooling | Editor side: cargo builds, compiler messages, `--oddity-describe` | `oddity_project`, `serde_json` |

## Build plan

Each task is one Claude Code session, maybe two. Do them in order: each one
builds on the last. A task is done when its checks pass, fmt and clippy are
clean, and its box here is ticked. The runtime tasks (A to C) don't touch the
editor and are tested headless.

The test game used throughout lives in `samples/shooter/`: the example from
the top of this file, built up as the tasks land.

### A · The runtime

- [x] **A1. Crates and `#[script]`.** Create `oddity`, `oddity_script`,
  `oddity_script_macros`. Implement `#[script]` (field rules, `#[default]`,
  `id`, `order`), the Inspector attributes (`range`, `min`, `tooltip`,
  `header`, `multiline`, stored as reflect custom attributes), the `Script`
  trait with every callback defaulted, and automatic registration through
  Bevy's `reflect_auto_register` with a `ReflectScript` type data and an
  `#[reflect(@Export)]` attribute on saved fields.
  *Done when:* a test declares two scripts, registers derived types, and finds
  both through `ReflectScript` with the right ID, exported fields, defaults and
  attributes.
- [x] **A2. The runner.** The stage systems, Hierarchy order, take-out/put-back,
  skipping unimplemented callbacks, `_onready`/`_onenable` timing,
  `DisabledScripts`, `destroy` with `_ondisable`/`_ondestroy`, panic catching.
  *Done when:* tests prove order (by hierarchy, then `order`), that `_onready`
  runs once before the first `_onprocess`, that changes are visible to the next
  script immediately, that a destroyed node's scripts stop, and that a
  panicking script is switched off while the others keep running.
- [x] **A3. `Ctx`: this node, components, finding, resources, time.**
  *Done when:* each `Ctx` method in those tables has a test.
- [x] **A4. `NodeRef` and other nodes.** `position_of`, `global_transform_of`,
  `call` with the re-entrancy queue, `all`, groups.
  *Done when:* the Enemy example's `call::<Health>` works in a test, and a
  self-call is queued and runs after the caller.
- [x] **A5. Events and listeners.** `#[listener]`, `Event`, `Event<T>`,
  `invoke`, `connect`, `disconnect`.
  *Done when:* Health's `on_died` reaches a listener on another node, and one
  on its own node (queued).
- [x] **A6. Signals.** `#[signal]`, `emit`, `#[on]`; systems read them with `MessageReader`.
- [x] **A7. Timers.** `after`, `every`, `cancel_timers`.
- [x] **A8. Input.** Raw input through `ctx.input()`, and the input map from
  `oddity.toml` (`action_*`, `axis`, `axis2`).
- [x] **A9. Pause and time.** `pause`, `set_time_scale`, process modes.
  *Done when:* while paused, a *Pausable* script stops and an *Always* one
  keeps running; `delta` follows the time scale.
- [x] **A10. Logging and tests.** Script logs carry their node; the
  `oddity::test::Game` harness. *Done when:* the test example above passes
  against a small level (built in code; `Game::load` arrives with B1).

### B · Files

- [x] **B0. Asset IDs.** `.import` files: create, read, keep in sync; the
  asset ID type used by `Prefab`, `AudioClip` and level files.
  *Done when:* creating, renaming and moving files through the API keeps every
  ID, and a lost `.import` file is recovered by path.
- [x] **B1. The level format.** `oddity_level`: write a level from a World,
  read one back, `NodeRef` ids, unknown components kept as-is.
  *Done when:* round-trip tests (save, load, compare) pass, including
  `NodeRef`s and nested structs, and `oddity::test::Game::load` runs the
  testing example against a level file.
- [x] **B2. Prefabs.** Prefab files, instances with overrides, the `Prefab`
  field type, `ctx.instantiate`.
  *Done when:* an instance with an override loads with the overridden value
  and every other value from the prefab; changing the prefab file changes
  every instance that didn't override it.

### C · The game program

- [x] **C1. `oddity::run`.** The runner entry point: `DefaultPlugins`, the
  script runtime, the level loader, the game's plugin; `--oddity-play <level>`,
  `--oddity-describe`. Without flags it plays `[editor] main_level`. Optional
  features (physics first) are Cargo features on `oddity`; `oddity::run` adds
  the plugin of each one that's on (see
  [modularity.md](modularity.md#features-in-games)). The level's first
  camera is switched on (levels save cameras off, for the editor).
  *Done when:* `samples/shooter` runs and is playable from the command line
  (`cargo run` in its folder: WASD moves, Space fires, goblins chase), and
  `--oddity-describe` prints the JSON above for its scripts. Until F, a
  bullet hits by distance; the HUD and sound join with G.
- [x] **C2. Project template.** `oddity_project` creates the game crate (with
  `oddity`'s default features), the `.oddity/runner` crate and the
  `.gitignore`, and sets the shared target folder.
  (`Project::create_game`; `write_runner` writes `.oddity/runner` and
  `.cargo/config.toml` again, as the editor does before a build, since both
  are this machine's and git-ignored. `cargo play` runs the game from a
  terminal.)
  *Done when:* a freshly created project builds and runs.

### D · The editor

Depends on the Inspector's editable fields from the Phase 3 Inspector track,
and on modularity tasks P2 and P4: script fields and reference fields are
drawn through the Inspector field registry in `oddity_plugin_api`.

Everything D and E add is a registered command
([commands.md](commands.md)), so it can be undone where that makes sense and
an agent can do it over MCP: `script.attach`, `script.remove`,
`script.set_enabled` and `script.set_field` (Edit),
`script.create`, `build.run`, `level.save`, `level.open`, `play.start` and
`play.stop` (Action), `build.errors` (Query).

- [x] **D1. Building from the editor.** `oddity_build`: file watching, cargo
  with JSON messages, clickable errors in Output (open the file at the line in
  VS Code or Zed), the hammer's states, describe after each successful build.
  (The watcher looks at `src/` and `Cargo.toml` a few times a second, with no
  file-event dependency; the editor also builds once on opening a game that
  has no `types.json`. Before each build it writes the runner again. The
  hammer glows while building and turns red when the build fails, and a
  failed build brings Output to the front. `code.open` opens a file at a
  line.)
- [x] **D2. Script values in the editor.** `ScriptInstance`, Inspector fields
  drawn from `types.json`, edits as undoable commands, the enable checkbox,
  Add Component listing scripts, dragging a script file onto a node.
  (`oddity_core::scripts`: a node's `Scripts`, the `ScriptTypes` read from
  `types.json`, and the `script.*` commands. An Inspector field edits a
  script's field through a `script:<type path>` `FieldTarget`. A file
  dropped on a node attaches the script named like it, or its only one;
  `--oddity-describe` gives each script's `file`.)
- [ ] **D3. Reference fields.** `NodeRef` (drag from the Hierarchy), `Prefab` and
  `AudioClip` (drag from the FileSystem), the `Event` listener editor (pick a
  node, then one of its listeners).
- [ ] **D4. New Script.** Attach Script / New Script from templates; open the
  file in VS Code or Zed.
- [ ] **D5. Saving and opening levels** through `oddity_level`, each open
  level with its own history (needs commands K5).
- [ ] **D6. Renames in the editor.** The FileSystem dock moves `.import` files
  along; broken references are listed with a re-link option; scripts without
  an `id` get an "Add ID" fix.

### E · Play

- [ ] **E1. Play and Stop.** Build if needed, start the runner, show its log in
  Output, report crashes, Stop. Embedded mode comes later.
- [ ] **E2. The live link.** `bevy_remote` in the runner when linked; a live
  Hierarchy and Inspector while playing; edits sent to the game. Edits made
  while playing never enter the level's history, and are gone on Stop.

### F · Physics

- [ ] **F1. Physics nodes and the Rapier backend.** `oddity_physics` (the
  nodes and the backend interface), `oddity_physics_rapier` (behind the
  `physics-rapier` Cargo feature on `oddity`, on by default) and
  `oddity_physics_editor` (registers the physics nodes through
  `oddity_plugin_api` and draws collision shapes in the viewport). Needs
  modularity P3. *Done when:* the physics nodes appear in Add Node, and
  `cargo xtask layers` confirms nothing but `oddity` depends on the Rapier crate.
- [ ] **F2. Collision and trigger callbacks, raycasts.** *Done when:* bullets in
  `samples/shooter` damage goblins, and a coin `Area3D` fires `_ontriggerenter`.

### G · Sound, UI, debug drawing

- [ ] **G1.** `AudioClip` and `play_sound`; UI Button nodes with an `on_pressed`
  event; `draw_line` and `draw_sphere`.

## Later

- 2D: `Node2D` physics nodes and a 2D view (right after the 3D path works).
- Hot-patching function bodies with Bevy's `hotpatching` feature, so changing
  code inside a callback needs no rebuild-and-restart of the game.
- Embedded Play inside the viewport.
- Waiting coroutines (`yield`-style), if `after`/`every` aren't enough.
- Switching the physics backend to Avian when it supports Bevy 0.20.
- Running script code in the editor (Unity's `[ExecuteInEditMode]`), through
  the live link.
