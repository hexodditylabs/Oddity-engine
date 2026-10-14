//! The level open in the editor.
//!
//! Levels load from `.level` files in Phase 3. Until then the editor starts
//! with a small demo level, so the Hierarchy and Inspector work on real
//! entities and every edit goes through real, undoable commands.

use bevy::prelude::*;
use oddity_core::LevelRoot;
use oddity_icons::Icon;

pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_demo_level);
    }
}

/// The icon a node shows in the Hierarchy.
///
/// Temporary: in Phase 3 the node-type registry ("Add Node" dialog) gives
/// each node type its icon, and this component goes away.
#[derive(Component, Clone, Debug)]
pub struct NodeIcon(pub Icon);

impl Default for NodeIcon {
    fn default() -> Self {
        Self(Icon::godot("Node"))
    }
}

/// A plain node for the demo level: a name and a Godot icon.
fn node(name: &str, icon: &'static str) -> impl Bundle {
    (Name::new(name.to_owned()), NodeIcon(Icon::godot(icon)))
}

/// A node that draws something, so it can be hidden (it gets the eye button).
fn visual(name: &str, icon: &'static str) -> impl Bundle {
    (node(name, icon), Visibility::default())
}

/// A visual node placed in 3D.
fn spatial(name: &str, icon: &'static str, at: Vec3) -> impl Bundle {
    (visual(name, icon), Transform::from_translation(at))
}

fn spawn_demo_level(mut commands: Commands) {
    commands
        .spawn((spatial("Main", "Node3D", Vec3::ZERO), LevelRoot))
        .with_children(|level| {
            level.spawn(spatial("Camera", "Camera3D", Vec3::new(0.0, 4.0, 8.0)));
            level.spawn(spatial(
                "Sun",
                "DirectionalLight3D",
                Vec3::new(0.0, 10.0, 0.0),
            ));
            level.spawn(node("Environment", "WorldEnvironment"));
            level
                .spawn(spatial("Player", "CharacterBody3D", Vec3::ZERO))
                .with_children(|player| {
                    player.spawn(spatial("Body", "MeshInstance3D", Vec3::ZERO));
                    player.spawn(spatial("Collision", "CollisionShape3D", Vec3::ZERO));
                    player.spawn(spatial(
                        "FollowCamera",
                        "Camera3D",
                        Vec3::new(0.0, 2.0, 4.0),
                    ));
                });
            level
                .spawn(spatial("Enemies", "Node3D", Vec3::ZERO))
                .with_children(|enemies| {
                    enemies.spawn(spatial(
                        "Goblin",
                        "CharacterBody3D",
                        Vec3::new(5.0, 0.0, -3.0),
                    ));
                    enemies.spawn(spatial(
                        "Goblin2",
                        "CharacterBody3D",
                        Vec3::new(-4.0, 0.0, -6.0),
                    ));
                });
            level.spawn(visual("HUD", "Control")).with_children(|hud| {
                hud.spawn(visual("HealthBar", "ProgressBar"));
            });
        });
}
