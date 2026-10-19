//! Signals (`roadmap/scripting.md`, "Signals"): game-wide messages.
//!
//! ```ignore
//! #[signal]
//! pub struct EnemyKilled { pub score: u32 }
//!
//! ctx.emit(EnemyKilled { score: 10 });
//!
//! #[script]
//! impl Scoreboard {
//!     #[on(EnemyKilled)]
//!     fn add_score(&mut self, ctx: &mut Ctx, killed: &EnemyKilled) { .. }
//! }
//!
//! fn system(mut kills: MessageReader<EnemyKilled>) { .. }   // systems read them too
//! ```
//!
//! `#[on]` handlers run right after the script that emitted finishes, on
//! every node that has one, in Hierarchy order. A signal is also written as
//! a Bevy message, so systems read it with `MessageReader`.

use std::{
    any::{Any, TypeId},
    sync::Arc,
};

use bevy::{ecs::message::Messages, prelude::*, reflect::CreateTypeData};

/// A game-wide signal. `#[signal]` implements it; scripts `emit` signals
/// and handle them with `#[on(..)]`.
pub trait Signal: Message + Clone + Reflect {}

/// Type data on every signal type, so [`ScriptPlugin`](crate::ScriptPlugin)
/// can add each one's message channel without a list to maintain.
#[derive(Clone)]
pub struct ReflectSignal {
    add_message: fn(&mut App),
}

impl ReflectSignal {
    /// Add this signal's message channel to `app`.
    pub(crate) fn add_message(&self, app: &mut App) {
        (self.add_message)(app);
    }
}

impl<T: Signal> CreateTypeData<T> for ReflectSignal {
    fn create_type_data(_input: ()) -> Self {
        Self {
            add_message: |app| {
                app.add_message::<T>();
            },
        }
    }
}

/// A signal emitted by a running script, waiting for it to finish.
pub(crate) struct PendingSignal {
    pub(crate) signal: TypeId,
    pub(crate) value: Arc<dyn Any + Send + Sync>,
    write: fn(&mut World, &(dyn Any + Send + Sync)),
}

impl PendingSignal {
    pub(crate) fn new<S: Signal>(signal: S) -> Self {
        Self {
            signal: TypeId::of::<S>(),
            value: Arc::new(signal),
            write: |world, value| {
                if let Some(signal) = value.downcast_ref::<S>()
                    && world.contains_resource::<Messages<S>>()
                {
                    world.write_message(signal.clone());
                }
            },
        }
    }

    /// Write it as a Bevy message, for systems.
    pub(crate) fn write(&self, world: &mut World) {
        (self.write)(world, &*self.value);
    }
}
