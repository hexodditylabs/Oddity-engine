//! Testing game code (`roadmap/scripting.md`, "Testing your game code"):
//! [`Game`] runs a level headless (no window, no GPU), so tests run in
//! `cargo test` and in CI.
//!
//! ```ignore
//! #[test]
//! fn goblins_die_when_shot() {
//!     let mut game = Game::load("levels/arena.level");
//!     let goblin = game.find("Enemies/Goblin");
//!     game.call::<Health>(goblin, |health, ctx| health.damage(1000.0, ctx));
//!     game.step(1);
//!     assert!(!game.exists(goblin));
//! }
//! ```
//!
//! A test can also build its level in code, with [`Game::with_level`].

use std::{path::Path, time::Duration};

use bevy::{
    app::Plugins, ecs::hierarchy::ChildSpawner, input::InputPlugin, prelude::*,
    time::TimeUpdateStrategy, transform::TransformPlugin,
};
use oddity_level::LevelPlugin;
use oddity_script::{Ctx, NodeRef, Script, ScriptPlugin};

/// A game running headless, a frame at a time. Every frame lasts
/// [`Game::FRAME`], so tests give the same result on every machine.
pub struct Game {
    app: App,
    level: NodeRef,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    /// How long a frame lasts: 1/64 s, Bevy's default fixed step, which
    /// the game is set to as well, so every frame runs `_onphysicsprocess`
    /// exactly once.
    pub const FRAME: Duration = Duration::from_micros(15_625);

    /// The engine, the scripts and no level. Nothing has run yet. Prefabs
    /// come from the crate being tested, as if it were the project folder,
    /// and so does the input map, when it has an `oddity.toml`.
    ///
    /// # Panics
    ///
    /// If that `oddity.toml` has a binding that names no key or button.
    pub fn new() -> Self {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            TransformPlugin,
            InputPlugin,
            ScriptPlugin,
            LevelPlugin::new("."),
        ))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Self::FRAME))
        .insert_resource(Time::<Fixed>::from_duration(Self::FRAME));
        if let Ok(project) = oddity_project::Project::load(".") {
            let input = crate::input_map(&project.file.input)
                .unwrap_or_else(|error| panic!("oddity.toml: {error}"));
            app.insert_resource(input);
        }
        Self {
            app,
            level: NodeRef::NONE,
        }
    }

    /// The engine and the scripts, started on the level file at `path`
    /// (relative to the crate being tested, where `cargo test` runs).
    ///
    /// # Panics
    ///
    /// If the level can't be read: a test can't go on without it.
    pub fn load(path: impl AsRef<Path>) -> Self {
        Self::new().with_level_file(path)
    }

    /// Add the game's own plugins: its systems and resources (scripts
    /// register themselves).
    pub fn add_plugins<M>(mut self, plugins: impl Plugins<M>) -> Self {
        self.app.add_plugins(plugins);
        self
    }

    /// Start the game on a level built by `build`, under a root node called
    /// `Level`. The first frame runs, as when a game starts: every script
    /// gets `_onready`, then its first `_onprocess` with a `delta` of 0.
    pub fn with_level(mut self, build: impl FnOnce(&mut ChildSpawner)) -> Self {
        let level = self
            .app
            .world_mut()
            .spawn((
                Name::new("Level"),
                Transform::default(),
                Visibility::default(),
            ))
            .with_children(build)
            .id();
        self.level = NodeRef::from(level);
        self.app.update();
        self
    }

    /// Start the game on the level file at `path`, as [`Game::load`] does,
    /// after [`add_plugins`](Self::add_plugins).
    ///
    /// # Panics
    ///
    /// If the level can't be read.
    pub fn with_level_file(mut self, path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let level = oddity_level::Level::read(path)
            .and_then(|level| oddity_level::spawn_level(self.app.world_mut(), &level))
            .unwrap_or_else(|error| panic!("can't load {}: {error}", path.display()));
        self.level = NodeRef::from(level);
        self.app.update();
        self
    }

    /// The level's root node.
    pub fn level(&self) -> NodeRef {
        self.level
    }

    /// The node at `path` below the level root, by names
    /// (`"Enemies/Goblin"`); [`NodeRef::NONE`] if there is none.
    pub fn find(&self, path: &str) -> NodeRef {
        let world = self.app.world();
        let mut at = self.level.entity();
        for name in path.split('/').filter(|name| !name.is_empty()) {
            at = at.and_then(|parent| {
                world
                    .get::<Children>(parent)?
                    .iter()
                    .find(|&child| world.get::<Name>(child).map(Name::as_str) == Some(name))
            });
        }
        at.map_or(NodeRef::NONE, NodeRef::from)
    }

    /// Run `f` on `node`'s script `S` now, with that node's `ctx`: what
    /// another script's `ctx.call` would do. Nothing happens if the node or
    /// the script isn't there.
    pub fn call<S: Script + Component>(
        &mut self,
        node: NodeRef,
        f: impl FnOnce(&mut S, &mut Ctx) + Send + Sync + 'static,
    ) {
        oddity_script::call(self.app.world_mut(), node, f);
    }

    /// Run `frames` frames.
    pub fn step(&mut self, frames: u32) {
        for _ in 0..frames {
            self.app.update();
        }
    }

    /// Whether `node` is still in the game.
    pub fn exists(&self, node: NodeRef) -> bool {
        node.entity()
            .is_some_and(|entity| self.app.world().get_entity(entity).is_ok())
    }

    /// `node`'s component (or script) `T`.
    pub fn get<T: Component>(&self, node: NodeRef) -> Option<&T> {
        self.app.world().get::<T>(node.entity()?)
    }

    /// The game's world, to read resources and anything else.
    pub fn world(&self) -> &World {
        self.app.world()
    }

    /// The game's world, to change: press keys, insert resources.
    pub fn world_mut(&mut self) -> &mut World {
        self.app.world_mut()
    }
}
