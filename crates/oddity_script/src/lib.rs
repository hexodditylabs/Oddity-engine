//! Oddity Engine's scripting runtime (`roadmap/scripting.md`).
//!
//! A script is a struct marked [`script`]: a Bevy component the Inspector
//! can edit and a level can save. Its behaviour is its [`Script`] callbacks,
//! which the runner calls one script at a time with a [`Ctx`].
//!
//! Games use all of this through the `oddity` crate's prelude, and the
//! [`ScriptPlugin`] runs it (see `runner`). `Ctx`'s methods, events,
//! signals and timers arrive with the following scripting tasks (A3 to A9).

// `#[script]` refers to this crate as `oddity_script` when used inside it.
extern crate self as oddity_script;

pub mod attributes;
mod ctx;
mod events;
mod input;
mod pause;
mod prefab;
mod runner;
mod signals;
mod timers;

use std::any::{Any, TypeId};

use bevy::{
    ecs::{
        change_detection::DetectChangesMut,
        component::{Component, Mutable},
        entity::Entity,
        reflect::ReflectComponent,
        world::EntityWorldMut,
    },
    reflect::{CreateTypeData, PartialReflect, Reflect, std_traits::ReflectDefault},
};

pub use ctx::Ctx;
pub use events::{Event, EventValue, Listener, NoValue};
pub use input::{Axis2Binding, Binding, BindingError, Input, InputMap, Stick};
pub use oddity_script_macros::{listener, on, script, signal};
pub use pause::ProcessMode;
pub use prefab::{Prefab, PrefabSpawner};
pub use runner::{DisabledScripts, ScriptPlugin, ScriptSystems, call, destroy};
pub use signals::{ReflectSignal, Signal};
pub use timers::{ScriptTimer, ScriptTimers};

/// A script's callbacks. All are optional; `delta` is in seconds.
///
/// Every `#[script]` type implements it, even if with nothing inside, so the
/// runner can call it:
///
/// ```ignore
/// impl Script for Health {
///     fn _onready(&mut self, _ctx: &mut Ctx) {
///         self.current = self.max;
///     }
/// }
/// ```
///
/// The collision and trigger callbacks arrive with physics (milestone F),
/// which defines what they receive.
///
/// The default bodies tell the runner the callback isn't implemented, so it
/// stops calling it for that script type.
pub trait Script: Any + Send + Sync {
    /// Once, before the node's first `_onprocess` (Unity's `Start`).
    fn _onready(&mut self, ctx: &mut Ctx) {
        ctx.unimplemented = true;
    }

    /// Every frame (Unity's `Update`).
    fn _onprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        ctx.unimplemented = true;
    }

    /// At the fixed physics rate (Unity's `FixedUpdate`).
    fn _onphysicsprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        ctx.unimplemented = true;
    }

    /// Every frame, after every `_onprocess` (Unity's `LateUpdate`).
    fn _onlateprocess(&mut self, ctx: &mut Ctx, _delta: f32) {
        ctx.unimplemented = true;
    }

    /// Before `_onready`, and whenever the script is switched back on.
    fn _onenable(&mut self, ctx: &mut Ctx) {
        ctx.unimplemented = true;
    }

    /// When switched off, and before `_ondestroy`.
    fn _ondisable(&mut self, ctx: &mut Ctx) {
        ctx.unimplemented = true;
    }

    /// Just before the node is removed.
    fn _ondestroy(&mut self, ctx: &mut Ctx) {
        ctx.unimplemented = true;
    }
}

/// A handle to a node, for script fields: drag a node from the Hierarchy
/// onto the field in the Inspector. [`NodeRef::NONE`] points nowhere.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
#[reflect(Default, Debug, PartialEq, Hash)]
pub struct NodeRef(Option<Entity>);

impl NodeRef {
    /// Points at no node.
    pub const NONE: Self = Self(None);

    /// Whether it points at no node.
    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }

    /// The node's entity, for plain Bevy code.
    pub fn entity(&self) -> Option<Entity> {
        self.0
    }
}

/// The groups a node is in (Unity's tags, Godot's groups). Scripts use
/// `ctx.add_to_group`, `ctx.remove_from_group` and `ctx.in_group`.
#[derive(Component, Reflect, Default, Debug, Clone, PartialEq, Eq)]
#[reflect(Component, Default)]
pub struct Groups(pub Vec<String>);

impl Groups {
    /// Whether the node is in `group`.
    pub fn contains(&self, group: &str) -> bool {
        self.0.iter().any(|name| name == group)
    }
}

/// A node from plain Bevy code, such as a system's query.
impl From<Entity> for NodeRef {
    fn from(entity: Entity) -> Self {
        Self(Some(entity))
    }
}

/// Type data on every script type: what the runner, the level files and the
/// Inspector need to know about it, and the way to reach it as a
/// [`Script`] through reflection. Added by `#[script]`.
#[derive(Clone)]
pub struct ReflectScript {
    id: Option<&'static str>,
    order: i32,
    file: (&'static str, &'static str),
    as_script: fn(&dyn Reflect) -> Option<&dyn Script>,
    as_script_mut: fn(&mut dyn Reflect) -> Option<&mut dyn Script>,
    take: fn(&mut EntityWorldMut) -> Option<Box<dyn Script>>,
    put_back: fn(&mut EntityWorldMut, Box<dyn Script>),
    reflect_mut: fn(&mut dyn Script) -> Option<&mut dyn Reflect>,
    listener_names: fn() -> Vec<&'static str>,
    listener_args: fn() -> Vec<Option<&'static str>>,
    call_listener: fn(&mut dyn Script, &mut Ctx, &str, Option<&dyn PartialReflect>) -> bool,
    handles: fn(TypeId) -> bool,
    call_handlers: fn(&mut dyn Script, &mut Ctx, TypeId, &dyn Any),
}

impl ReflectScript {
    /// The stable ID from `#[script(id = "...")]`, which levels store.
    /// Without one, levels store the type's Rust path.
    pub fn id(&self) -> Option<&'static str> {
        self.id
    }

    /// The file the script is in, for the editor to attach it when the
    /// file is dropped on a node: absolute, or joined to the crate's folder.
    pub fn file(&self) -> std::path::PathBuf {
        let (file, crate_dir) = self.file;
        let file = std::path::Path::new(file);
        if file.is_absolute() {
            file.to_path_buf()
        } else {
            std::path::Path::new(crate_dir).join(file)
        }
    }

    /// Its `order`: lower runs earlier in each stage. 0 by default.
    pub fn order(&self) -> i32 {
        self.order
    }

    /// `value` as a script, if it is one of this type.
    pub fn get<'a>(&self, value: &'a dyn Reflect) -> Option<&'a dyn Script> {
        (self.as_script)(value)
    }

    /// `value` as a script to call, if it is one of this type.
    pub fn get_mut<'a>(&self, value: &'a mut dyn Reflect) -> Option<&'a mut dyn Script> {
        (self.as_script_mut)(value)
    }

    /// The names of its `#[listener]` methods, for the Inspector and
    /// `connect`.
    pub fn listener_names(&self) -> Vec<&'static str> {
        (self.listener_names)()
    }

    /// Its `#[listener]` methods with the type path of the value each
    /// takes (`None` for one that takes none), for the game's type
    /// description.
    pub fn listeners(&self) -> Vec<(&'static str, Option<&'static str>)> {
        self.listener_names()
            .into_iter()
            .zip((self.listener_args)())
            .collect()
    }

    /// Whether it has a `#[listener]` method called `name`.
    pub fn has_listener(&self, name: &str) -> bool {
        self.listener_names().contains(&name)
    }

    /// Call its listener `name` on `script`, with `value` for listeners that
    /// take one. `false` if there is no such listener.
    pub(crate) fn call_listener(
        &self,
        script: &mut dyn Script,
        ctx: &mut Ctx,
        name: &str,
        value: Option<&dyn PartialReflect>,
    ) -> bool {
        (self.call_listener)(script, ctx, name, value)
    }

    /// Whether it has an `#[on(..)]` handler for the signal type `signal`.
    pub fn handles(&self, signal: TypeId) -> bool {
        (self.handles)(signal)
    }

    /// Call its handlers of the signal type `signal` on `script`.
    pub(crate) fn call_handlers(
        &self,
        script: &mut dyn Script,
        ctx: &mut Ctx,
        signal: TypeId,
        value: &dyn Any,
    ) {
        (self.call_handlers)(script, ctx, signal, value);
    }

    /// A taken-out script as a reflected value, to reach its fields.
    pub(crate) fn reflect_mut<'a>(
        &self,
        script: &'a mut dyn Script,
    ) -> Option<&'a mut dyn Reflect> {
        (self.reflect_mut)(script)
    }

    /// Take the script out of its component for a call, leaving its default
    /// in place: no archetype change, and no change detection.
    pub(crate) fn take(&self, entity: &mut EntityWorldMut) -> Option<Box<dyn Script>> {
        (self.take)(entity)
    }

    /// Put a taken script back.
    pub(crate) fn put_back(&self, entity: &mut EntityWorldMut, script: Box<dyn Script>) {
        (self.put_back)(entity, script);
    }
}

impl<T> CreateTypeData<T> for ReflectScript
where
    T: Script + Reflect + Default + Component<Mutability = Mutable> + __private::ScriptMeta,
{
    fn create_type_data(_input: ()) -> Self {
        Self {
            id: T::ID,
            order: T::ORDER,
            file: (T::FILE, T::CRATE_DIR),
            as_script: |value| value.downcast_ref::<T>().map(|s| s as &dyn Script),
            as_script_mut: |value| value.downcast_mut::<T>().map(|s| s as &mut dyn Script),
            take: |entity| {
                let mut component = entity.get_mut::<T>()?;
                let script = std::mem::take(component.bypass_change_detection());
                Some(Box::new(script))
            },
            put_back: |entity, script| {
                let Ok(script) = (script as Box<dyn Any>).downcast::<T>() else {
                    return;
                };
                if let Some(mut component) = entity.get_mut::<T>() {
                    *component.bypass_change_detection() = *script;
                }
            },
            reflect_mut: |script| {
                (script as &mut dyn Any)
                    .downcast_mut::<T>()
                    .map(|script| script as &mut dyn Reflect)
            },
            listener_names: || T::LISTENERS.iter().map(|entry| entry.name).collect(),
            listener_args: || {
                T::LISTENERS
                    .iter()
                    .map(|entry| entry.arg.map(|arg| arg()))
                    .collect()
            },
            call_listener: |script, ctx, name, value| {
                let Some(script) = (script as &mut dyn Any).downcast_mut::<T>() else {
                    return false;
                };
                let Some(entry) = T::LISTENERS.iter().find(|entry| entry.name == name) else {
                    return false;
                };
                (entry.call)(script, ctx, value);
                true
            },
            handles: |signal| T::HANDLERS.iter().any(|entry| (entry.signal)() == signal),
            call_handlers: |script, ctx, signal, value| {
                let Some(script) = (script as &mut dyn Any).downcast_mut::<T>() else {
                    return;
                };
                for entry in T::HANDLERS
                    .iter()
                    .filter(|entry| (entry.signal)() == signal)
                {
                    (entry.call)(script, ctx, value);
                }
            },
        }
    }
}

/// What `#[script]`'s generated code refers to. Not part of the API.
#[doc(hidden)]
pub mod __private {
    pub use bevy::{
        ecs::{component::Component, message::Message, reflect::ReflectComponent},
        reflect::{PartialReflect, Reflect, TypePath, std_traits::ReflectDefault},
    };

    pub use crate::runner::{script_added, script_removed};

    use crate::Ctx;

    /// A script type's settings from `#[script(..)]`.
    pub trait ScriptMeta: Sized + 'static {
        /// The file the script is in, as the compiler names it: relative to
        /// the crate's workspace, or absolute for a crate outside it (a
        /// game, built from its runner).
        const FILE: &'static str;
        /// The crate's folder, which a relative [`FILE`](Self::FILE) is in
        /// when the crate is its own workspace, as a game is.
        const CRATE_DIR: &'static str;
        /// `id = "..."`.
        const ID: Option<&'static str>;
        /// `order = n`.
        const ORDER: i32;
        /// Its `#[listener]` methods, from `#[script] impl`.
        const LISTENERS: &'static [ListenerEntry<Self>];
        /// Its `#[on(..)]` signal handlers, from `#[script] impl`.
        const HANDLERS: &'static [HandlerEntry<Self>];
    }

    /// One `#[on(..)]` method: the signal type it handles, and how to call
    /// it with a signal.
    pub struct HandlerEntry<T> {
        /// The signal's type.
        pub signal: fn() -> std::any::TypeId,
        /// Calls the method.
        pub call: fn(&mut T, &mut Ctx, &dyn std::any::Any),
    }

    /// One `#[listener]` method: its name, and how to call it with an
    /// event's value, if it takes one.
    pub struct ListenerEntry<T> {
        /// The method's name.
        pub name: &'static str,
        /// The type path of the value it takes, if it takes one.
        pub arg: Option<fn() -> &'static str>,
        /// Calls the method.
        pub call: fn(&mut T, &mut Ctx, Option<&dyn PartialReflect>),
    }

    /// The tables of a script without `#[script] impl`: empty. The inherent
    /// constants `#[script] impl` makes take precedence over these.
    pub trait EmptyTables: Sized + 'static {
        /// No listeners.
        const __ODDITY_LISTENERS: &'static [ListenerEntry<Self>] = &[];
        /// No signal handlers.
        const __ODDITY_HANDLERS: &'static [HandlerEntry<Self>] = &[];
    }

    impl<T: 'static> EmptyTables for T {}
}
