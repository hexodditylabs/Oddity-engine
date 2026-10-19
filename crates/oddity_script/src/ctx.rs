//! [`Ctx`]: everything a script does to the game goes through it
//! (`roadmap/scripting.md`, "`Ctx`").
//!
//! The runner builds one for each callback, holding the world and the node.

use bevy::{
    ecs::{change_detection::Mut, component::ComponentId, component::Mutable},
    prelude::*,
};

use bevy::reflect::PartialReflect;

use crate::{
    Groups, Input, Listener, NodeRef, Prefab, PrefabSpawner, Script, Signal,
    runner::{call_script, deliver, hierarchy_key, script_with_event},
    signals::PendingSignal,
    timers::{ScriptTimer, ScriptTimers},
};

/// What the runner does once the current script has returned.
pub(crate) enum After {
    /// Switch a script on this node on or off.
    Enable { script: ComponentId, on: bool },
}

/// Everything a script does to the game goes through its `ctx`: its own
/// node, other nodes, resources, time. The runner builds one for each call.
pub struct Ctx<'w> {
    world: &'w mut World,
    node: Entity,
    /// Set by a default callback body: the script type doesn't have it.
    pub(crate) unimplemented: bool,
    /// Set by [`Ctx::destroy`]: remove the node once the script returns.
    pub(crate) destroy: bool,
    pub(crate) after: Vec<After>,
    /// Signals emitted during this call, for after it.
    pub(crate) signals: Vec<PendingSignal>,
}

impl<'w> Ctx<'w> {
    pub(crate) fn new(world: &'w mut World, node: Entity) -> Self {
        Self {
            world,
            node,
            unimplemented: false,
            destroy: false,
            after: Vec::new(),
            signals: Vec::new(),
        }
    }

    /// This node.
    pub fn node(&self) -> NodeRef {
        NodeRef::from(self.node)
    }

    /// This node's name; empty if it has none.
    pub fn name(&self) -> &str {
        self.world.get::<Name>(self.node).map_or("", Name::as_str)
    }

    /// This node's position, relative to its parent.
    pub fn position(&self) -> Vec3 {
        self.transform().translation
    }

    /// Move this node to `position`, relative to its parent.
    pub fn set_position(&mut self, position: Vec3) {
        self.transform_mut().translation = position;
    }

    /// Move this node by `offset`.
    pub fn translate(&mut self, offset: Vec3) {
        self.transform_mut().translation += offset;
    }

    /// This node's rotation, relative to its parent.
    pub fn rotation(&self) -> Quat {
        self.transform().rotation
    }

    /// Turn this node to `rotation`, relative to its parent.
    pub fn set_rotation(&mut self, rotation: Quat) {
        self.transform_mut().rotation = rotation;
    }

    /// Turn this node to face `target`, given in the same space as
    /// [`Ctx::position`], with up as up. Nothing happens if it is already
    /// there.
    pub fn look_at(&mut self, target: Vec3) {
        let mut transform = self.transform_mut();
        if transform.translation != target {
            transform.look_at(target, Vec3::Y);
        }
    }

    /// The way this node faces: its local -Z, as in Godot and Bevy.
    pub fn forward(&self) -> Vec3 {
        *self.transform().forward()
    }

    /// This node's transform, relative to its parent. Nodes without one
    /// read as the identity.
    pub fn transform(&self) -> Transform {
        self.get::<Transform>().copied().unwrap_or_default()
    }

    /// This node's transform, to change. A node without one gets one.
    pub fn transform_mut(&mut self) -> Mut<'_, Transform> {
        if self.world.get::<Transform>(self.node).is_none() {
            self.world
                .entity_mut(self.node)
                .insert(Transform::default());
        }
        self.world
            .get_mut::<Transform>(self.node)
            .expect("the node has a transform now")
    }

    /// Where this node is in the world, as of the last transform update.
    pub fn global_transform(&self) -> GlobalTransform {
        self.get::<GlobalTransform>().copied().unwrap_or_default()
    }

    /// A component on this node (Unity's `GetComponent`). The running
    /// script itself is reached through `self`, not this.
    pub fn get<T: Component>(&self) -> Option<&T> {
        self.world.get::<T>(self.node)
    }

    /// A component on this node, to change.
    pub fn get_mut<T: Component<Mutability = Mutable>>(&mut self) -> Option<Mut<'_, T>> {
        self.world.get_mut::<T>(self.node)
    }

    /// Switch the script `S` on this node on or off, once the current script
    /// returns. Switching off runs its `_ondisable`, switching back on its
    /// `_onenable`, if it has started.
    pub fn enable<S: Script + Component>(&mut self, on: bool) {
        if let Some(script) = self.world.components().component_id::<S>() {
            self.after.push(After::Enable { script, on });
        }
    }

    /// Whether `node` has a `T`.
    pub fn has<T: Component>(&self, node: NodeRef) -> bool {
        self.get_on::<T>(node).is_some()
    }

    /// A component on another node.
    pub fn get_on<T: Component>(&self, node: NodeRef) -> Option<&T> {
        self.world.get::<T>(node.entity()?)
    }

    /// A component on another node, to change.
    pub fn get_mut_on<T: Component<Mutability = Mutable>>(
        &mut self,
        node: NodeRef,
    ) -> Option<Mut<'_, T>> {
        self.world.get_mut::<T>(node.entity()?)
    }

    /// This node's parent, or [`NodeRef::NONE`] at the top.
    pub fn parent(&self) -> NodeRef {
        parent_of(self.world, self.node).map_or(NodeRef::NONE, NodeRef::from)
    }

    /// This node's children, in Hierarchy order.
    pub fn children(&self) -> Vec<NodeRef> {
        self.world
            .get::<Children>(self.node)
            .map(|children| children.iter().map(NodeRef::from).collect())
            .unwrap_or_default()
    }

    /// The node at `path`, Godot-style: names separated by `/`, relative to
    /// this node; `..` is the parent, and a leading `/` starts at the top of
    /// the Hierarchy. [`NodeRef::NONE`] if there is no such node.
    ///
    /// `find("Muzzle")`, `find("../Enemies/Goblin")`, `find("/Main/HUD")`.
    pub fn find(&self, path: &str) -> NodeRef {
        let (mut at, path) = match path.strip_prefix('/') {
            Some(rest) => {
                // The top of the Hierarchy holds the root, so the first name
                // must be the root's own.
                let mut root = self.node;
                while let Some(parent) = parent_of(self.world, root) {
                    root = parent;
                }
                let (first, rest) = rest.split_once('/').unwrap_or((rest, ""));
                if self.world.get::<Name>(root).map(Name::as_str) != Some(first) {
                    return NodeRef::NONE;
                }
                (root, rest)
            }
            None => (self.node, path),
        };
        for step in path
            .split('/')
            .filter(|step| !step.is_empty() && *step != ".")
        {
            let next = if step == ".." {
                parent_of(self.world, at)
            } else {
                child_named(self.world, at, step)
            };
            match next {
                Some(next) => at = next,
                None => return NodeRef::NONE,
            }
        }
        NodeRef::from(at)
    }

    /// The child called `name`, or [`NodeRef::NONE`].
    pub fn find_child(&self, name: &str) -> NodeRef {
        child_named(self.world, self.node, name).map_or(NodeRef::NONE, NodeRef::from)
    }

    /// Where `node` is in the world, as of the last transform update;
    /// `None` if the node is gone.
    pub fn position_of(&self, node: NodeRef) -> Option<Vec3> {
        self.global_transform_of(node)
            .map(|transform| transform.translation())
    }

    /// `node`'s place in the world, as of the last transform update; `None`
    /// if the node is gone.
    pub fn global_transform_of(&self, node: NodeRef) -> Option<GlobalTransform> {
        let entity = self.world.get_entity(node.entity()?).ok()?;
        Some(entity.get::<GlobalTransform>().copied().unwrap_or_default())
    }

    /// Run `f` on `node`'s script `S`, with that node's own `ctx`:
    ///
    /// ```ignore
    /// let damage = self.damage;
    /// ctx.call::<Health>(target, move |health, ctx| health.damage(damage, ctx));
    /// ```
    ///
    /// It runs now, unless that script is running higher up the call stack
    /// (a script calling itself, say): then it runs as soon as that script
    /// finishes. That's why the closure is `move`: it may outlive this call.
    /// Nothing happens if the node or the script isn't there.
    pub fn call<S: Script + Component>(
        &mut self,
        node: NodeRef,
        f: impl FnOnce(&mut S, &mut Ctx) + Send + Sync + 'static,
    ) {
        crate::call(self.world, node, f);
    }

    /// Every node with the script `S`, in Hierarchy order (Unity's
    /// `FindObjectsOfType`).
    pub fn all<S: Script + Component>(&mut self) -> Vec<NodeRef> {
        let nodes: Vec<Entity> = self
            .world
            .query_filtered::<Entity, With<S>>()
            .iter(self.world)
            .collect();
        in_hierarchy_order(self.world, nodes)
    }

    /// Every node in `group`, in Hierarchy order (Unity's tags, Godot's
    /// groups).
    pub fn in_group(&mut self, group: &str) -> Vec<NodeRef> {
        let nodes: Vec<Entity> = self
            .world
            .query::<(Entity, &Groups)>()
            .iter(self.world)
            .filter(|(_, groups)| groups.contains(group))
            .map(|(entity, _)| entity)
            .collect();
        in_hierarchy_order(self.world, nodes)
    }

    /// Listen to the `Event` field called `event` on `source` (on whichever
    /// of its scripts has it) with this node's `#[listener]` called
    /// `listener`. Listening twice the same way changes nothing.
    ///
    /// `ctx.connect(me, "on_died", "die")`
    pub fn connect(&mut self, source: NodeRef, event: &str, listener: &str) {
        let entry = Listener {
            node: self.node(),
            name: listener.to_owned(),
        };
        self.edit_listeners(source, event, move |listeners| {
            if !listeners.contains(&entry) {
                listeners.push(entry);
            }
        });
    }

    /// Stop listening, undoing [`Ctx::connect`] with the same arguments.
    pub fn disconnect(&mut self, source: NodeRef, event: &str, listener: &str) {
        let entry = Listener {
            node: self.node(),
            name: listener.to_owned(),
        };
        self.edit_listeners(source, event, move |listeners| {
            listeners.retain(|existing| *existing != entry);
        });
    }

    /// Change the listener list of `source`'s event `event`, through `call`
    /// so a running script's own event is changed when it is back.
    fn edit_listeners(
        &mut self,
        source: NodeRef,
        event: &str,
        edit: impl FnOnce(&mut Vec<Listener>) + Send + Sync + 'static,
    ) {
        let Some(node) = source.entity() else {
            return;
        };
        let Some((script, script_type)) = script_with_event(self.world, node, event) else {
            warn!("No script on {node} has an event called `{event}`");
            return;
        };
        let path = format!("{event}.listeners");
        let call = Box::new(move |taken: &mut dyn Script, _ctx: &mut Ctx| {
            let listeners = script_type
                .reflect_mut(taken)
                .and_then(|script| script.reflect_path_mut(path.as_str()).ok())
                .and_then(|field| field.try_downcast_mut::<Vec<Listener>>());
            if let Some(listeners) = listeners {
                edit(listeners);
            }
        });
        call_script(self.world, node, script, call);
    }

    /// Call one listener of an event, with the event's value if it has one.
    pub(crate) fn deliver(&mut self, listener: &Listener, value: Option<Box<dyn PartialReflect>>) {
        if let Some(node) = listener.node.entity() {
            deliver(self.world, node, &listener.name, value);
        }
    }

    /// Call this node's `#[listener]` called `listener` once, `seconds` from
    /// now (Unity's `Invoke`).
    pub fn after(&mut self, seconds: f32, listener: &str) {
        self.add_timer(seconds, listener, None);
    }

    /// Call this node's `#[listener]` called `listener` every `seconds`,
    /// starting `seconds` from now (Unity's `InvokeRepeating`).
    pub fn every(&mut self, seconds: f32, listener: &str) {
        self.add_timer(seconds, listener, Some(seconds.max(0.0)));
    }

    /// Stop every timer on this node.
    pub fn cancel_timers(&mut self) {
        if let Some(mut timers) = self.world.get_mut::<ScriptTimers>(self.node) {
            timers.0.clear();
        }
    }

    fn add_timer(&mut self, seconds: f32, listener: &str, every: Option<f32>) {
        let timer = ScriptTimer {
            listener: listener.to_owned(),
            remaining: seconds,
            every,
        };
        self.world
            .entity_mut(self.node)
            .entry::<ScriptTimers>()
            .or_default()
            .get_mut()
            .0
            .push(timer);
    }

    /// Send a game-wide signal. Once this script finishes, every
    /// `#[on(..)]` handler of it runs, in Hierarchy order, and systems can
    /// read it with `MessageReader`.
    pub fn emit<S: Signal>(&mut self, signal: S) {
        self.signals.push(PendingSignal::new(signal));
    }

    /// Put this node in `group`.
    pub fn add_to_group(&mut self, group: &str) {
        let mut entity = self.world.entity_mut(self.node);
        let mut groups = entity.entry::<Groups>().or_default();
        if !groups.get().contains(group) {
            groups.get_mut().0.push(group.to_owned());
        }
    }

    /// Take this node out of `group`.
    pub fn remove_from_group(&mut self, group: &str) {
        if let Some(mut groups) = self.world.get_mut::<Groups>(self.node) {
            groups.0.retain(|name| name != group);
        }
    }

    /// A Bevy resource (Unity's singletons, Godot's autoloads).
    ///
    /// # Panics
    ///
    /// If there is none, which switches the script off with that message.
    pub fn resource<R: Resource>(&self) -> &R {
        self.world.resource::<R>()
    }

    /// A Bevy resource, to change.
    ///
    /// # Panics
    ///
    /// If there is none, which switches the script off with that message.
    pub fn resource_mut<R: Resource<Mutability = Mutable>>(&mut self) -> Mut<'_, R> {
        self.world.resource_mut::<R>()
    }

    /// What the player is pressing: keys, mouse, and the project's actions
    /// and axes (`oddity_script::Input`).
    pub fn input(&self) -> Input<'_> {
        Input::new(self.world)
    }

    /// Seconds since the last frame, times the time scale: the fixed step
    /// in `_onphysicsprocess`. While paused, the real frame time, for the
    /// scripts that still run.
    pub fn delta(&self) -> f32 {
        crate::pause::script_delta(self.world)
    }

    /// Pause or resume the game. While paused, game time, physics and
    /// timers stand still, and only nodes whose process mode is *When
    /// paused* or *Always* run their per-frame callbacks; scripts later
    /// this frame already see it.
    pub fn pause(&mut self, paused: bool) {
        if let Some(mut time) = self.world.get_resource_mut::<Time<Virtual>>() {
            if paused {
                time.pause();
            } else {
                time.unpause();
            }
        }
    }

    /// Whether the game is paused.
    pub fn is_paused(&self) -> bool {
        crate::pause::is_paused(self.world)
    }

    /// Run game time at `scale` times real time from the next frame: 0.5
    /// for slow motion. `delta`, timers and physics follow it.
    ///
    /// # Panics
    ///
    /// If `scale` is negative or not finite.
    pub fn set_time_scale(&mut self, scale: f32) {
        if let Some(mut time) = self.world.get_resource_mut::<Time<Virtual>>() {
            time.set_relative_speed(scale);
        }
    }

    /// The time scale: 1 is real time.
    pub fn time_scale(&self) -> f32 {
        self.world
            .get_resource::<Time<Virtual>>()
            .map_or(1.0, Time::<Virtual>::relative_speed)
    }

    /// Seconds since the game started.
    pub fn time(&self) -> f32 {
        self.world
            .get_resource::<Time>()
            .map_or(0.0, Time::elapsed_secs)
    }

    /// Spawn `prefab` in this node's level (as a child of its root), with
    /// its root at `transform` relative to the level root. A
    /// `GlobalTransform` works too (`ctx.global_transform_of(muzzle)`):
    /// the level root normally sits at the origin. Its scripts get
    /// `_onready` before their first `_onprocess`, this frame.
    ///
    /// # Panics
    ///
    /// If the prefab can't be spawned (no such file, or it doesn't read),
    /// which switches the script off with the reason.
    pub fn instantiate(&mut self, prefab: &Prefab, transform: impl Into<Transform>) -> NodeRef {
        let transform = transform.into();
        let Some(spawner) = self.world.get_resource::<PrefabSpawner>().copied() else {
            panic!(
                "can't instantiate {:?}: no prefab spawner (add oddity_level's LevelPlugin)",
                prefab.path
            );
        };
        let mut root = self.node;
        while let Some(parent) = parent_of(self.world, root) {
            root = parent;
        }
        match (spawner.0)(self.world, prefab, transform, Some(root)) {
            Ok(entity) => NodeRef::from(entity),
            Err(why) => panic!("can't instantiate {:?}: {why}", prefab.path),
        }
    }

    /// Remove this node and everything under it once this callback
    /// returns, running their scripts' `_ondisable` and `_ondestroy` first.
    pub fn destroy(&mut self) {
        self.destroy = true;
    }

    /// The escape hatch to plain Bevy.
    pub fn world_mut(&mut self) -> &mut World {
        self.world
    }
}

fn in_hierarchy_order(world: &World, mut nodes: Vec<Entity>) -> Vec<NodeRef> {
    nodes.sort_by_cached_key(|node| hierarchy_key(world, *node));
    nodes.into_iter().map(NodeRef::from).collect()
}

fn parent_of(world: &World, node: Entity) -> Option<Entity> {
    world.get::<ChildOf>(node).map(ChildOf::parent)
}

fn child_named(world: &World, node: Entity, name: &str) -> Option<Entity> {
    world
        .get::<Children>(node)?
        .iter()
        .find(|child| world.get::<Name>(*child).map(Name::as_str) == Some(name))
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use bevy::time::TimeUpdateStrategy;

    use super::*;

    /// Run `f` with a `Ctx` for `node`, as a script callback would.
    fn with_ctx<R>(world: &mut World, node: Entity, f: impl FnOnce(&mut Ctx) -> R) -> R {
        f(&mut Ctx::new(world, node))
    }

    /// Main ─┬─ Player ── Muzzle
    ///       └─ Enemies ── Goblin
    fn level() -> (World, [Entity; 5]) {
        let mut world = World::new();
        let main = world.spawn(Name::new("Main")).id();
        let player = world
            .spawn((
                Name::new("Player"),
                Transform::from_xyz(1.0, 2.0, 3.0),
                ChildOf(main),
            ))
            .id();
        let muzzle = world.spawn((Name::new("Muzzle"), ChildOf(player))).id();
        let enemies = world.spawn((Name::new("Enemies"), ChildOf(main))).id();
        let goblin = world.spawn((Name::new("Goblin"), ChildOf(enemies))).id();
        (world, [main, player, muzzle, enemies, goblin])
    }

    #[test]
    fn node_and_name() {
        let (mut world, [_, player, ..]) = level();
        let (node, name) = with_ctx(&mut world, player, |ctx| {
            (ctx.node(), ctx.name().to_owned())
        });
        assert_eq!(node, NodeRef::from(player));
        assert_eq!(name, "Player");
    }

    #[test]
    fn position_set_position_and_translate() {
        let (mut world, [_, player, ..]) = level();
        with_ctx(&mut world, player, |ctx| {
            assert_eq!(ctx.position(), Vec3::new(1.0, 2.0, 3.0));
            ctx.set_position(Vec3::ZERO);
            ctx.translate(Vec3::X * 2.0);
            assert_eq!(ctx.position(), Vec3::new(2.0, 0.0, 0.0));
        });
        assert_eq!(world.get::<Transform>(player).unwrap().translation.x, 2.0);
    }

    #[test]
    fn rotation_look_at_and_forward() {
        let (mut world, [_, player, ..]) = level();
        with_ctx(&mut world, player, |ctx| {
            ctx.set_rotation(Quat::from_rotation_y(FRAC_PI_2));
            assert!(
                ctx.rotation()
                    .angle_between(Quat::from_rotation_y(FRAC_PI_2))
                    < 1e-3
            );
            // Facing -Z after a quarter turn left means facing -X.
            assert!(ctx.forward().abs_diff_eq(Vec3::NEG_X, 1e-5));

            ctx.look_at(ctx.position() + Vec3::Z);
            assert!(ctx.forward().abs_diff_eq(Vec3::Z, 1e-5));
            ctx.look_at(ctx.position());
            assert!(
                ctx.forward().is_finite(),
                "looking at itself changes nothing"
            );
        });
    }

    #[test]
    fn transform_transform_mut_and_global_transform() {
        let (mut world, [_, player, muzzle, ..]) = level();
        world
            .entity_mut(player)
            .insert(GlobalTransform::from_xyz(5.0, 0.0, 0.0));
        with_ctx(&mut world, player, |ctx| {
            assert_eq!(ctx.transform().translation, Vec3::new(1.0, 2.0, 3.0));
            ctx.transform_mut().scale = Vec3::splat(2.0);
            assert_eq!(ctx.transform().scale, Vec3::splat(2.0));
            assert_eq!(
                ctx.global_transform().translation(),
                Vec3::new(5.0, 0.0, 0.0)
            );
        });
        // A node without a transform reads as the identity and gets one when changed.
        with_ctx(&mut world, muzzle, |ctx| {
            assert_eq!(ctx.transform(), Transform::IDENTITY);
            ctx.translate(Vec3::Y);
        });
        assert!(world.get::<Transform>(muzzle).is_some());
    }

    #[derive(Component, Debug, PartialEq)]
    struct Ammo(u32);

    #[test]
    fn get_and_get_mut_on_this_node() {
        let (mut world, [_, player, ..]) = level();
        world.entity_mut(player).insert(Ammo(3));
        with_ctx(&mut world, player, |ctx| {
            assert_eq!(ctx.get::<Ammo>(), Some(&Ammo(3)));
            ctx.get_mut::<Ammo>().unwrap().0 -= 1;
            assert!(ctx.get::<Name>().is_some());
        });
        assert_eq!(world.get::<Ammo>(player), Some(&Ammo(2)));
    }

    #[test]
    fn has_get_on_and_get_mut_on_other_nodes() {
        let (mut world, [_, player, _, _, goblin]) = level();
        world.entity_mut(goblin).insert(Ammo(10));
        with_ctx(&mut world, player, |ctx| {
            let goblin = ctx.find("../Enemies/Goblin");
            assert!(ctx.has::<Ammo>(goblin));
            assert!(!ctx.has::<Ammo>(ctx.node()));
            assert!(!ctx.has::<Ammo>(NodeRef::NONE));
            assert_eq!(ctx.get_on::<Ammo>(goblin), Some(&Ammo(10)));
            ctx.get_mut_on::<Ammo>(goblin).unwrap().0 = 0;
        });
        assert_eq!(world.get::<Ammo>(goblin), Some(&Ammo(0)));
    }

    #[test]
    fn parent_and_children() {
        let (mut world, [main, player, muzzle, enemies, _]) = level();
        with_ctx(&mut world, player, |ctx| {
            assert_eq!(ctx.parent(), NodeRef::from(main));
            assert_eq!(ctx.children(), [NodeRef::from(muzzle)]);
        });
        with_ctx(&mut world, main, |ctx| {
            assert!(ctx.parent().is_none());
            assert_eq!(
                ctx.children(),
                [NodeRef::from(player), NodeRef::from(enemies)]
            );
        });
    }

    #[test]
    fn find_and_find_child() {
        let (mut world, [main, player, muzzle, _, goblin]) = level();
        with_ctx(&mut world, player, |ctx| {
            assert_eq!(ctx.find("Muzzle"), NodeRef::from(muzzle));
            assert_eq!(ctx.find("../Enemies/Goblin"), NodeRef::from(goblin));
            assert_eq!(ctx.find("/Main/Enemies/Goblin"), NodeRef::from(goblin));
            assert_eq!(ctx.find("/Main"), NodeRef::from(main));
            assert_eq!(ctx.find("."), ctx.node());
            assert!(ctx.find("Nobody").is_none());
            assert!(ctx.find("/Elsewhere/Goblin").is_none());
            assert_eq!(ctx.find_child("Muzzle"), NodeRef::from(muzzle));
            assert!(ctx.find_child("Goblin").is_none(), "only direct children");
        });
    }

    #[derive(Resource, Default)]
    struct Score(u32);

    #[test]
    fn resource_and_resource_mut() {
        let (mut world, [_, player, ..]) = level();
        world.init_resource::<Score>();
        with_ctx(&mut world, player, |ctx| {
            ctx.resource_mut::<Score>().0 += 5;
            assert_eq!(ctx.resource::<Score>().0, 5);
        });
    }

    #[test]
    fn delta_and_time() {
        let mut app = App::new();
        app.add_plugins(bevy::time::TimePlugin).insert_resource(
            TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(250)),
        );
        app.update();
        app.update();
        let node = app.world_mut().spawn_empty().id();
        with_ctx(app.world_mut(), node, |ctx| {
            assert_eq!(ctx.delta(), 0.25);
            assert_eq!(ctx.time(), 0.25, "the first frame has no time to measure");
        });
    }
}
