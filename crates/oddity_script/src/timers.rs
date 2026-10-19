//! Timers (`roadmap/scripting.md`, "Events, signals, timers"): Unity's
//! `Invoke` and `InvokeRepeating`.
//!
//! ```ignore
//! ctx.after(2.0, "expire");    // call this node's `expire` listener once, in 2 s
//! ctx.every(0.5, "blink");     // and `blink` every half second
//! ctx.cancel_timers();         // stop this node's timers
//! ```
//!
//! A node's timers live in its [`ScriptTimers`] component; the runner ticks
//! them in `Update`, before scripts' `_onprocess`, and calls the listener
//! when one fires, the way an event calls it. They follow the node's
//! process mode: a pausable node's timers stand still while paused.

use bevy::{prelude::*, reflect::std_traits::ReflectDefault};

use crate::{
    pause::can_process,
    runner::{deliver, hierarchy_key},
};

/// One timer: the listener it calls, and when.
#[derive(Reflect, Clone, Debug, PartialEq)]
pub struct ScriptTimer {
    /// The `#[listener]` to call, on whichever of the node's scripts has it.
    pub listener: String,
    /// Seconds until it fires.
    pub remaining: f32,
    /// For a repeating timer, the seconds between calls.
    pub every: Option<f32>,
}

/// A node's timers, set with `ctx.after` and `ctx.every`.
#[derive(Component, Reflect, Default, Debug, Clone, PartialEq)]
#[reflect(Component, Default)]
pub struct ScriptTimers(pub Vec<ScriptTimer>);

/// Count every node's timers down by `delta` and call the listeners of
/// those that are due: nodes in Hierarchy order, each node's timers in the
/// order they were set.
pub(crate) fn tick(world: &mut World, delta: f32) {
    let mut due: Vec<(Vec<u32>, Entity, Vec<String>)> = Vec::new();
    let mut nodes = world.query::<(Entity, &ScriptTimers)>();
    let running: Vec<Entity> = nodes
        .iter(world)
        .map(|(node, _)| node)
        .filter(|&node| can_process(world, node))
        .collect();
    for node in running {
        let Some(mut timers) = world.get_mut::<ScriptTimers>(node) else {
            continue;
        };
        let mut fired = Vec::new();
        timers.0.retain_mut(|timer| {
            timer.remaining -= delta;
            if timer.remaining > 0.0 {
                return true;
            }
            fired.push(timer.listener.clone());
            match timer.every {
                // At most once a frame, however long the frame was; an
                // interval of zero means every frame.
                Some(every) => {
                    timer.remaining = (timer.remaining + every).max(0.0);
                    true
                }
                None => false,
            }
        });
        if !fired.is_empty() {
            due.push((Vec::new(), node, fired));
        }
    }
    for (key, node, _) in &mut due {
        *key = hierarchy_key(world, *node);
    }
    due.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, node, listeners) in due {
        for listener in listeners {
            if world.get_entity(node).is_err() {
                break;
            }
            deliver(world, node, &listener, None);
        }
    }
}
