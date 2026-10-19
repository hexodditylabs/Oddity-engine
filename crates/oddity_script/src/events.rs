//! Events and listeners (`roadmap/scripting.md`, "Events and listeners"):
//! Unity's `UnityEvent`, wired in the Inspector or from code.
//!
//! ```ignore
//! pub on_died: Event,             // no data
//! pub on_damaged: Event<f32>,     // carries a value
//!
//! self.on_died.invoke(ctx);
//! self.on_damaged.invoke(ctx, amount);
//! ```
//!
//! An event holds `(node, listener name)` pairs, which levels save as plain
//! data. Invoking one calls each listener through `Ctx::call`, so a
//! listener whose script is running higher up the call stack is queued and
//! runs as soon as it finishes.

use std::marker::PhantomData;

use bevy::{prelude::*, reflect::PartialReflect};

use crate::{Ctx, NodeRef};

/// One listener of an event: a `#[listener]` method, by name, on whichever
/// script of `node` has one by that name.
#[derive(Reflect, Clone, Debug, PartialEq, Eq)]
pub struct Listener {
    /// The node whose script listens.
    pub node: NodeRef,
    /// The `#[listener]` method's name.
    pub name: String,
}

/// The value type of an [`Event`] that carries none. Deliberately not
/// `Clone`, which keeps it out of `Event<T>`'s two-argument `invoke`.
#[derive(Reflect, Debug, Default)]
pub struct NoValue;

/// What an `Event<T>` can carry: anything reflectable that can be copied to
/// each listener.
pub trait EventValue: Clone + PartialReflect {}

impl<T: Clone + PartialReflect> EventValue for T {}

/// An event other scripts listen to. `Event` carries no value;
/// `Event<f32>` carries one to each listener.
#[derive(Reflect)]
#[reflect(Default)]
pub struct Event<T: TypePath + Send + Sync + 'static = NoValue> {
    listeners: Vec<Listener>,
    #[reflect(ignore)]
    value: PhantomData<fn(T) -> T>,
}

impl<T: TypePath + Send + Sync + 'static> Event<T> {
    /// Who listens, in the order they will be called.
    pub fn listeners(&self) -> &[Listener] {
        &self.listeners
    }
}

impl Event {
    /// Call every listener now.
    pub fn invoke(&self, ctx: &mut Ctx) {
        for listener in &self.listeners {
            ctx.deliver(listener, None);
        }
    }
}

impl<T: EventValue + TypePath> Event<T> {
    /// Call every listener now, each with its own copy of `value`.
    pub fn invoke(&self, ctx: &mut Ctx, value: T) {
        for listener in &self.listeners {
            ctx.deliver(listener, Some(Box::new(value.clone())));
        }
    }
}

// By hand: derived ones would require `T` to be `Default`/`Clone`/..., and
// `NoValue` deliberately isn't `Clone`.
impl<T: TypePath + Send + Sync + 'static> Default for Event<T> {
    fn default() -> Self {
        Self {
            listeners: Vec::new(),
            value: PhantomData,
        }
    }
}

impl<T: TypePath + Send + Sync + 'static> Clone for Event<T> {
    fn clone(&self) -> Self {
        Self {
            listeners: self.listeners.clone(),
            value: PhantomData,
        }
    }
}

impl<T: TypePath + Send + Sync + 'static> std::fmt::Debug for Event<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Event")
            .field("listeners", &self.listeners)
            .finish()
    }
}

impl<T: TypePath + Send + Sync + 'static> PartialEq for Event<T> {
    fn eq(&self, other: &Self) -> bool {
        self.listeners == other.listeners
    }
}
