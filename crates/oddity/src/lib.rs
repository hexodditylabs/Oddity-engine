//! Oddity Engine for games: the one crate a game depends on.
//!
//! ```ignore
//! use oddity::prelude::*;
//!
//! #[script]
//! pub struct Spinner {
//!     #[default(90.0)]
//!     pub degrees_per_second: f32,
//! }
//!
//! impl Script for Spinner {}
//! ```
//!
//! A file with scripts needs `use oddity::prelude::*;`: besides the names
//! you write, it brings in the two Bevy names `#[script]`'s derives refer
//! to. Without it the error is "cannot find module or crate `bevy_ecs`".
//!
//! A game's runner calls [`run`] with the game's plugin. Optional features
//! (physics, Steam, networking) arrive behind Cargo features as they are
//! built.

mod describe;
mod input;
mod run;
pub mod test;

pub use bevy;
pub use describe::describe;
pub use input::{InputMapError, input_map};
pub use oddity_script::{
    Axis2Binding, Binding, BindingError, Ctx, DisabledScripts, Event, EventValue, Groups, Input,
    InputMap, Listener, NoValue, NodeRef, Prefab, ProcessMode, ReflectScript, ReflectSignal,
    Script, ScriptPlugin, ScriptSystems, ScriptTimer, ScriptTimers, Signal, Stick, attributes,
    call, destroy, listener, on, script, signal,
};
pub use run::{Options, run};

/// What `#[script]`'s generated code refers to. Not part of the API.
#[doc(hidden)]
pub use oddity_script::__private;

/// Everything a script file needs: `use oddity::prelude::*;`.
pub mod prelude {
    pub use bevy::prelude::*;
    // Named explicitly, so `Event` here is the script event, not the Bevy
    // observer event the glob above also brings in.
    pub use oddity_script::{
        Ctx, Event, NodeRef, Prefab, Script, Signal, listener, on, script, signal,
    };

    // Bevy's derives name `bevy_ecs` and `bevy_reflect` relative to where
    // they are used, when the crate doesn't depend on `bevy` itself (Bevy's
    // `BevyManifest` falls back to the bare crate name). A game depends only
    // on `oddity`, so `#[script]` finds them here.
    #[doc(hidden)]
    pub use bevy::{ecs as bevy_ecs, reflect as bevy_reflect};
}
