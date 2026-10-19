//! The level open in the editor.
//!
//! Levels load from `.level` files in Phase 3. Until then the editor starts
//! with a small demo level, so the Hierarchy and Inspector work on real
//! entities and every edit goes through real, undoable commands. Its nodes
//! carry their node type's name, which picks their icon, but not that
//! type's components, so nothing in it renders.

use bevy::prelude::*;
use oddity_core::{LevelRoot, NodeKind};

/// Opens the demo level at startup.
pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_demo_level);
    }
}

/// A plain node for the demo level: a name and its node type.
fn node(name: &str, kind: &str) -> impl Bundle {
    (Name::new(name.to_owned()), NodeKind(kind.to_owned()))
}

/// A node that draws something, so it can be hidden (it gets the eye button).
fn visual(name: &str, kind: &str) -> impl Bundle {
    (node(name, kind), Visibility::default())
}

/// A visual node placed in 3D.
fn spatial(name: &str, kind: &str, at: Vec3) -> impl Bundle {
    (visual(name, kind), Transform::from_translation(at))
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
