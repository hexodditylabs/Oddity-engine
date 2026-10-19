//! The runner (`roadmap/scripting.md`, "The runner"): one exclusive system
//! per stage calls every script, one at a time, so a change one script makes
//! is visible to the next immediately.
//!
//! - Nodes run in Hierarchy order; within a node, scripts by `order`, then
//!   in the order they were added.
//! - Each script is taken out of its component for its call (no archetype
//!   change, no change detection) and put back after, if the node is still
//!   there.
//! - `_onenable` and `_onready` run before a script's first stage callback.
//! - A callback left at its default is never called again for that type.
//! - A panicking script is put back, listed in [`DisabledScripts`] and
//!   reported; everything else keeps running.

use std::{
    cell::RefCell,
    panic::{self, AssertUnwindSafe},
    sync::Once,
};

use bevy::{
    ecs::{component::ComponentId, lifecycle::HookContext, world::DeferredWorld},
    platform::collections::{HashMap, HashSet},
    prelude::*,
    reflect::std_traits::ReflectDefault,
};

use crate::{
    Ctx, NodeRef, ReflectScript, Script,
    ctx::After,
    signals::{PendingSignal, ReflectSignal},
};

/// A node's switched-off scripts, by type path. The runner skips them.
#[derive(Component, Reflect, Default, Debug, Clone, PartialEq, Eq)]
#[reflect(Component, Default)]
pub struct DisabledScripts(pub Vec<String>);

impl DisabledScripts {
    /// Whether the script of type `type_path` is switched off.
    pub fn contains(&self, type_path: &str) -> bool {
        self.0.iter().any(|path| path == type_path)
    }
}

/// The stages scripts run in, for ordering game systems around them.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScriptSystems {
    /// `_onphysicsprocess`, in `FixedUpdate`.
    PhysicsProcess,
    /// `_onprocess`, in `Update`.
    Process,
    /// `_onlateprocess`, in `PostUpdate` before transforms propagate.
    LateProcess,
}

/// Which callback to call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Callback {
    Enable,
    Ready,
    Process,
    PhysicsProcess,
    LateProcess,
    Disable,
    Destroy,
}

/// What the runner keeps between calls.
#[derive(Resource, Default)]
pub(crate) struct Runner {
    /// Every node's scripts, in the order they were added.
    scripts: HashMap<Entity, Vec<ComponentId>>,
    /// Scripts whose `_onready` has run.
    ready: HashSet<(Entity, ComponentId)>,
    /// Callbacks a script type left at the default.
    unimplemented: HashSet<(ComponentId, Callback)>,
    /// Nodes whose destroy callbacks are running.
    destroying: HashSet<Entity>,
    /// Scripts taken out of their components right now, innermost last.
    running: Vec<(Entity, ComponentId)>,
    /// Calls to a running script, waiting for it to finish.
    queued: Vec<(Entity, ComponentId, ScriptCall)>,
}

/// `#[script]`'s `on_add` hook: note the script, in the order added.
pub fn script_added(mut world: DeferredWorld, context: HookContext) {
    if let Some(mut runner) = world.get_resource_mut::<Runner>() {
        runner
            .scripts
            .entry(context.entity)
            .or_default()
            .push(context.component_id);
    }
}

/// `#[script]`'s `on_remove` hook: forget the script.
pub fn script_removed(mut world: DeferredWorld, context: HookContext) {
    if let Some(mut runner) = world.get_resource_mut::<Runner>() {
        if let Some(scripts) = runner.scripts.get_mut(&context.entity) {
            scripts.retain(|script| *script != context.component_id);
            if scripts.is_empty() {
                runner.scripts.remove(&context.entity);
            }
        }
        runner.ready.remove(&(context.entity, context.component_id));
    }
}

/// Runs scripts, and catches their panics.
pub struct ScriptPlugin;

impl Plugin for ScriptPlugin {
    fn build(&self, app: &mut App) {
        remember_panic_locations();
        // Every `#[signal]` registers itself; give each its message channel.
        let registry = app.world().resource::<AppTypeRegistry>().clone();
        let signals: Vec<ReflectSignal> = registry
            .read()
            .iter_with_data::<ReflectSignal>()
            .map(|(_, signal)| signal.clone())
            .collect();
        for signal in signals {
            signal.add_message(app);
        }
        app.init_resource::<Runner>()
            .init_resource::<crate::InputMap>()
            .add_systems(
                FixedUpdate,
                run_physics_process.in_set(ScriptSystems::PhysicsProcess),
            )
            .add_systems(Update, run_process.in_set(ScriptSystems::Process))
            .add_systems(
                PostUpdate,
                run_late_process
                    .in_set(ScriptSystems::LateProcess)
                    .before(bevy::transform::TransformSystems::Propagate),
            );
    }
}

fn run_physics_process(world: &mut World) {
    run_stage(world, Callback::PhysicsProcess);
}

fn run_process(world: &mut World) {
    crate::timers::tick(world, crate::pause::script_delta(world));
    run_stage(world, Callback::Process);
}

fn run_late_process(world: &mut World) {
    run_stage(world, Callback::LateProcess);
}

/// Run `f` on `node`'s script `S`, with that node's `ctx`, from outside
/// any script: a test, or a system through `commands.queue`. As with
/// `Ctx::call`, nothing happens if the node or the script isn't there, and
/// a call to a script that is running waits until it finishes.
pub fn call<S: Script + Component>(
    world: &mut World,
    node: NodeRef,
    f: impl FnOnce(&mut S, &mut Ctx) + Send + Sync + 'static,
) {
    let (Some(entity), Some(script)) = (node.entity(), world.components().component_id::<S>())
    else {
        return;
    };
    let call = Box::new(move |taken: &mut dyn Script, ctx: &mut Ctx| {
        if let Some(script) = (taken as &mut dyn std::any::Any).downcast_mut::<S>() {
            f(script, ctx);
        }
    });
    call_script(world, entity, script, call);
}

/// Remove `entity` the way scripts expect: its scripts' `_ondisable` and
/// `_ondestroy` run first, its children's too. Use this from systems; a
/// plain despawn skips the callbacks.
pub fn destroy(commands: &mut Commands, entity: Entity) {
    commands.queue(move |world: &mut World| destroy_now(world, entity));
}

fn run_stage(world: &mut World, callback: Callback) {
    let delta = crate::pause::script_delta(world);
    for (node, script) in run_order(world) {
        if !is_live(world, node, script) {
            continue;
        }
        if !world.resource::<Runner>().ready.contains(&(node, script)) {
            world.resource_mut::<Runner>().ready.insert((node, script));
            call_callback(world, node, script, Callback::Enable, delta);
            call_callback(world, node, script, Callback::Ready, delta);
        }
        // Checked per script: one that pauses the game stops the
        // pausable scripts after it this frame.
        if crate::pause::can_process(world, node) {
            call_callback(world, node, script, callback, delta);
        }
    }
}

/// Every script to run, in order: nodes in Hierarchy order, then each
/// node's scripts by `order`, then in the order they were added.
fn run_order(world: &World) -> Vec<(Entity, ComponentId)> {
    let runner = world.resource::<Runner>();
    let mut nodes: Vec<(Vec<u32>, Entity)> = runner
        .scripts
        .keys()
        .map(|&node| (hierarchy_key(world, node), node))
        .collect();
    nodes.sort_unstable();
    let mut order = Vec::new();
    for (_, node) in nodes {
        let mut scripts = runner.scripts[&node].clone();
        // Stable, so equal orders keep the order they were added in.
        scripts.sort_by_key(|script| script_type(world, *script).map_or(0, |t| t.order()));
        order.extend(scripts.into_iter().map(|script| (node, script)));
    }
    order
}

/// Where a node is in the Hierarchy, as a sort key: its root, then its
/// index among its siblings at each level down. Roots go in spawn order.
pub(crate) fn hierarchy_key(world: &World, node: Entity) -> Vec<u32> {
    let mut key = Vec::new();
    let mut current = node;
    while let Some(parent) = world.get::<ChildOf>(current).map(ChildOf::parent) {
        let index = world
            .get::<Children>(parent)
            .and_then(|children| children.iter().position(|child| child == current))
            .unwrap_or(usize::MAX);
        key.push(u32::try_from(index).unwrap_or(u32::MAX));
        current = parent;
    }
    key.push(current.index_u32());
    key.reverse();
    key
}

/// The script type registered for a component, if it is a script.
fn script_type(world: &World, script: ComponentId) -> Option<ReflectScript> {
    let type_id = world.components().get_info(script)?.type_id()?;
    let registry = world.get_resource::<AppTypeRegistry>()?.read();
    registry.get_type_data::<ReflectScript>(type_id).cloned()
}

/// Call `node`'s `#[listener]` called `name`, on whichever of its scripts
/// has one, through `call` (so a running script's listener is queued).
pub(crate) fn deliver(
    world: &mut World,
    node: Entity,
    name: &str,
    value: Option<Box<dyn bevy::reflect::PartialReflect>>,
) {
    let Some((script, script_type)) = script_with_listener(world, node, name) else {
        warn!("No script on {node} has a listener called `{name}`");
        return;
    };
    let name = name.to_owned();
    let call = Box::new(move |taken: &mut dyn Script, ctx: &mut Ctx| {
        script_type.call_listener(taken, ctx, &name, value.as_deref());
    });
    call_script(world, node, script, call);
}

/// The first of `node`'s scripts, in the order added, with a `#[listener]`
/// called `name`.
pub(crate) fn script_with_listener(
    world: &World,
    node: Entity,
    name: &str,
) -> Option<(ComponentId, ReflectScript)> {
    node_scripts(world, node).find(|(_, script_type)| script_type.has_listener(name))
}

/// The first of `node`'s scripts, in the order added, with an `Event` field
/// called `field`. Found from the type, so it works while the script is out
/// running.
pub(crate) fn script_with_event(
    world: &World,
    node: Entity,
    field: &str,
) -> Option<(ComponentId, ReflectScript)> {
    let registry = world.get_resource::<AppTypeRegistry>()?.read();
    type AnyEvent = crate::Event<crate::NoValue>;
    node_scripts(world, node).find(|(script, _)| {
        let Some(type_id) = world
            .components()
            .get_info(*script)
            .and_then(|i| i.type_id())
        else {
            return false;
        };
        registry
            .get(type_id)
            .and_then(|registration| registration.type_info().as_struct().ok())
            .and_then(|info| info.field(field))
            .is_some_and(|field| {
                let path = field.type_path_table();
                path.ident() == AnyEvent::type_ident()
                    && path.module_path() == AnyEvent::module_path()
            })
    })
}

fn node_scripts(
    world: &World,
    node: Entity,
) -> impl Iterator<Item = (ComponentId, ReflectScript)> + '_ {
    world
        .resource::<Runner>()
        .scripts
        .get(&node)
        .into_iter()
        .flatten()
        .filter_map(move |script| Some((*script, script_type(world, *script)?)))
}

/// A script's type path, as level files and [`DisabledScripts`] name it.
fn type_path(world: &World, script: ComponentId) -> String {
    let registered = world
        .components()
        .get_info(script)
        .and_then(|info| info.type_id())
        .and_then(|type_id| {
            let registry = world.get_resource::<AppTypeRegistry>()?.read();
            Some(registry.get(type_id)?.type_info().type_path().to_owned())
        });
    registered.unwrap_or_else(|| "a script".to_owned())
}

/// Whether the node and script still exist and the script is switched on.
fn is_live(world: &World, node: Entity, script: ComponentId) -> bool {
    let Ok(entity) = world.get_entity(node) else {
        return false;
    };
    if !entity.contains_id(script) {
        return false;
    }
    let path = type_path(world, script);
    !entity
        .get::<DisabledScripts>()
        .is_some_and(|disabled| disabled.contains(&path))
}

/// Call one callback of one script: take it out, call it with a `Ctx`,
/// put it back, then act on what happened.
fn call_callback(
    world: &mut World,
    node: Entity,
    script: ComponentId,
    callback: Callback,
    delta: f32,
) {
    run_script(world, node, script, Action::Callback(callback, delta));
}

/// Code to run on a script through `Ctx::call`, with its node's `Ctx`.
pub(crate) type ScriptCall = Box<dyn FnOnce(&mut dyn Script, &mut Ctx) + Send + Sync>;

/// What to do with a script once it is taken out.
enum Action {
    /// One of its callbacks.
    Callback(Callback, f32),
    /// Code from `Ctx::call`.
    Call(ScriptCall),
}

/// Run `call` on the script, now, or once it finishes if it is running
/// higher up the call stack (its data is out of its component until then).
pub(crate) fn call_script(world: &mut World, node: Entity, script: ComponentId, call: ScriptCall) {
    run_script(world, node, script, Action::Call(call));
}

/// Take the script out, act on it with a `Ctx`, put it back, then do what
/// it asked for.
fn run_script(world: &mut World, node: Entity, script: ComponentId, action: Action) {
    let callback = match &action {
        Action::Callback(callback, _) => Some(*callback),
        Action::Call(_) => None,
    };
    match callback {
        // The runner leaves switched-off scripts alone; `call` still
        // reaches them, as Unity's method calls do.
        Some(callback) => {
            if world
                .resource::<Runner>()
                .unimplemented
                .contains(&(script, callback))
                || !is_live(world, node, script)
            {
                return;
            }
        }
        None => {
            if !world
                .get_entity(node)
                .is_ok_and(|entity| entity.contains_id(script))
            {
                return;
            }
        }
    }
    if let Action::Call(call) = action {
        if world.resource::<Runner>().running.contains(&(node, script)) {
            world
                .resource_mut::<Runner>()
                .queued
                .push((node, script, call));
            return;
        }
        return run_taken(world, node, script, Action::Call(call), None);
    }
    run_taken(world, node, script, action, callback);
}

fn run_taken(
    world: &mut World,
    node: Entity,
    script: ComponentId,
    action: Action,
    callback: Option<Callback>,
) {
    let Some(script_type) = script_type(world, script) else {
        return;
    };
    let Some(mut taken) = world
        .get_entity_mut(node)
        .ok()
        .and_then(|mut entity| script_type.take(&mut entity))
    else {
        return;
    };

    world.resource_mut::<Runner>().running.push((node, script));
    // Everything the script logs carries its node (`roadmap/scripting.md`,
    // "Errors, logging and debugging"). An error-level span stays on
    // whatever the log level; its fields are only worked out when a log
    // subscriber is listening.
    let span = error_span!(
        "script",
        node = %world.get::<Name>(node).map_or("", Name::as_str),
        entity = %node,
        script = %type_path(world, script),
    );
    let entered = span.enter();
    let mut ctx = Ctx::new(world, node);
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| match action {
        Action::Callback(callback, delta) => invoke(&mut *taken, &mut ctx, callback, delta),
        Action::Call(call) => call(&mut *taken, &mut ctx),
    }));
    drop(entered);
    let (unimplemented, destroy) = (ctx.unimplemented, ctx.destroy);
    let after = std::mem::take(&mut ctx.after);
    let signals = std::mem::take(&mut ctx.signals);
    world.resource_mut::<Runner>().running.pop();

    if let Ok(mut entity) = world.get_entity_mut(node) {
        script_type.put_back(&mut entity, taken);
    }
    if let (true, Some(callback)) = (unimplemented, callback) {
        world
            .resource_mut::<Runner>()
            .unimplemented
            .insert((script, callback));
    }
    if let Err(panic) = outcome {
        switch_off_after_panic(world, node, script, &*panic);
        // What a panicking script asked for is dropped with it.
        return;
    }
    for action in after {
        match action {
            After::Enable { script, on } => set_enabled(world, node, script, on),
        }
    }
    run_queued(world, node, script);
    for signal in signals {
        dispatch(world, signal);
    }
    if destroy {
        destroy_now(world, node);
    }
}

/// Deliver a signal: as a message for systems, then to every `#[on(..)]`
/// handler of a switched-on script, in Hierarchy order.
fn dispatch(world: &mut World, signal: PendingSignal) {
    signal.write(world);
    let handlers: Vec<(Entity, ComponentId, ReflectScript)> = run_order(world)
        .into_iter()
        .filter(|(node, script)| is_live(world, *node, *script))
        .filter_map(|(node, script)| {
            let script_type = script_type(world, script)?;
            script_type
                .handles(signal.signal)
                .then_some((node, script, script_type))
        })
        .collect();
    for (node, script, script_type) in handlers {
        let (kind, value) = (signal.signal, signal.value.clone());
        let call = Box::new(move |taken: &mut dyn Script, ctx: &mut Ctx| {
            script_type.call_handlers(taken, ctx, kind, &*value);
        });
        call_script(world, node, script, call);
    }
}

/// Run the calls that waited for this script to finish, in order.
fn run_queued(world: &mut World, node: Entity, script: ComponentId) {
    loop {
        let next = {
            let mut runner = world.resource_mut::<Runner>();
            let index = runner
                .queued
                .iter()
                .position(|(n, s, _)| (*n, *s) == (node, script));
            index.map(|index| runner.queued.remove(index).2)
        };
        let Some(call) = next else {
            return;
        };
        call_script(world, node, script, call);
    }
}

/// Switch a script on a node on or off, with its callback if it has started.
fn set_enabled(world: &mut World, node: Entity, script: ComponentId, on: bool) {
    let Ok(entity) = world.get_entity(node) else {
        return;
    };
    if !entity.contains_id(script) {
        return;
    }
    let path = type_path(world, script);
    let is_off = entity
        .get::<DisabledScripts>()
        .is_some_and(|disabled| disabled.contains(&path));
    let started = world.resource::<Runner>().ready.contains(&(node, script));
    if on && is_off {
        if let Some(mut disabled) = world.get_mut::<DisabledScripts>(node) {
            disabled.0.retain(|disabled| *disabled != path);
        }
        if started {
            call_callback(world, node, script, Callback::Enable, 0.0);
        }
    } else if !on && !is_off {
        if started {
            call_callback(world, node, script, Callback::Disable, 0.0);
        }
        world
            .entity_mut(node)
            .entry::<DisabledScripts>()
            .or_default()
            .get_mut()
            .0
            .push(path);
    }
}

fn invoke(script: &mut dyn Script, ctx: &mut Ctx, callback: Callback, delta: f32) {
    match callback {
        Callback::Enable => script._onenable(ctx),
        Callback::Ready => script._onready(ctx),
        Callback::Process => script._onprocess(ctx, delta),
        Callback::PhysicsProcess => script._onphysicsprocess(ctx, delta),
        Callback::LateProcess => script._onlateprocess(ctx, delta),
        Callback::Disable => script._ondisable(ctx),
        Callback::Destroy => script._ondestroy(ctx),
    }
}

/// Run the destroy callbacks of `node` and everything under it, then
/// despawn it. Only scripts that have started (had `_onready`) get them,
/// as in Unity: there is nothing to undo for the others.
pub(crate) fn destroy_now(world: &mut World, node: Entity) {
    if world.get_entity(node).is_err() || !world.resource_mut::<Runner>().destroying.insert(node) {
        return;
    }
    let mut nodes = vec![node];
    let mut index = 0;
    while let Some(&current) = nodes.get(index) {
        if let Some(children) = world.get::<Children>(current) {
            nodes.extend(children.iter());
        }
        index += 1;
    }
    for current in nodes {
        let scripts = world
            .resource::<Runner>()
            .scripts
            .get(&current)
            .cloned()
            .unwrap_or_default();
        for script in scripts {
            if world
                .resource::<Runner>()
                .ready
                .contains(&(current, script))
            {
                call_callback(world, current, script, Callback::Disable, 0.0);
                call_callback(world, current, script, Callback::Destroy, 0.0);
            }
        }
    }
    if let Ok(entity) = world.get_entity_mut(node) {
        entity.despawn();
    }
    world.resource_mut::<Runner>().destroying.remove(&node);
}

fn switch_off_after_panic(
    world: &mut World,
    node: Entity,
    script: ComponentId,
    panic: &(dyn std::any::Any + Send),
) {
    let path = type_path(world, script);
    let message = panic
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "(no message)".to_owned());
    let location = LAST_PANIC_LOCATION
        .with_borrow_mut(Option::take)
        .unwrap_or_else(|| "an unknown place".to_owned());
    let name = world
        .get::<Name>(node)
        .map_or_else(|| format!("{node}"), |name| name.as_str().to_owned());
    error!("{path} on {name} panicked at {location}: {message}. It is switched off on that node.");
    if let Ok(mut entity) = world.get_entity_mut(node) {
        entity
            .entry::<DisabledScripts>()
            .or_default()
            .get_mut()
            .0
            .push(path);
    }
}

thread_local! {
    /// Where the last panic on this thread happened, for the report.
    static LAST_PANIC_LOCATION: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Keep each panic's file:line for the runner's report, then do whatever
/// panics did before (print it, usually).
fn remember_panic_locations() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let location = info
                .location()
                .map(|at| format!("{}:{}", at.file(), at.line()));
            LAST_PANIC_LOCATION.with_borrow_mut(|last| *last = location);
            previous(info);
        }));
    });
}
