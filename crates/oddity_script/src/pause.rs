//! Pause and time scale (`roadmap/scripting.md`, "Pause and time").
//!
//! Both live on Bevy's virtual clock (`Time<Virtual>`): pausing it stops
//! game time, the fixed step and with it physics; its relative speed is the
//! time scale. A node's [`ProcessMode`] says whether its scripts keep
//! running while paused, as in Godot.

use std::time::Duration;

use bevy::{prelude::*, reflect::std_traits::ReflectDefault};

/// Whether a node's scripts run while the game is paused. Set per node in
/// the Inspector; children inherit it.
#[derive(Component, Reflect, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[reflect(Component, Default)]
pub enum ProcessMode {
    /// Whatever the parent does; *Pausable* at the top of the level.
    #[default]
    Inherit,
    /// Runs while the game runs, stops while it's paused.
    Pausable,
    /// Runs only while the game is paused: a pause menu.
    WhenPaused,
    /// Runs either way.
    Always,
    /// Never runs `_onprocess`, `_onphysicsprocess` or `_onlateprocess`.
    Disabled,
}

/// The mode `node` actually has: its own, or the nearest ancestor's that
/// isn't *Inherit*.
pub(crate) fn resolved_mode(world: &World, node: Entity) -> ProcessMode {
    let mut current = node;
    loop {
        match world.get::<ProcessMode>(current) {
            Some(ProcessMode::Inherit) | None => {}
            Some(&mode) => return mode,
        }
        match world.get::<ChildOf>(current) {
            Some(child_of) => current = child_of.parent(),
            None => return ProcessMode::Pausable,
        }
    }
}

/// Whether the game is paused, including by a script earlier this frame.
pub(crate) fn is_paused(world: &World) -> bool {
    world
        .get_resource::<Time<Virtual>>()
        .is_some_and(Time::<Virtual>::is_paused)
}

/// Whether `node`'s scripts run their per-frame callbacks now.
pub(crate) fn can_process(world: &World, node: Entity) -> bool {
    match resolved_mode(world, node) {
        ProcessMode::Inherit | ProcessMode::Pausable => !is_paused(world),
        ProcessMode::WhenPaused => is_paused(world),
        ProcessMode::Always => true,
        ProcessMode::Disabled => false,
    }
}

/// The `delta` scripts get this frame. Paused game time stands still, so
/// the scripts that still run (a pause menu) get the real frame time, with
/// the same time scale and cap as game time.
pub(crate) fn script_delta(world: &World) -> f32 {
    let Some(virtual_time) = world.get_resource::<Time<Virtual>>() else {
        return world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    };
    if !virtual_time.was_paused() {
        return world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    }
    let real = world
        .get_resource::<Time<Real>>()
        .map_or(Duration::ZERO, Time::delta);
    real.min(virtual_time.max_delta()).as_secs_f32() * virtual_time.relative_speed()
}
