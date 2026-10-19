//! The built-in node types (`roadmap/scripting.md`, "Built-in nodes"),
//! registered through `oddity_plugin_api` the same way an extension adds its
//! own. Add Node lists them; `node.create` makes them.

use bevy::prelude::*;
use oddity_icons::Icon;
use oddity_plugin_api::{EditorAppExt, NodeType};

/// Node3D, MeshInstance3D, Camera3D and the three lights.
pub struct NodesPlugin;

impl Plugin for NodesPlugin {
    fn build(&self, app: &mut App) {
        app.register_node(
            NodeType::new::<(Transform, Visibility)>("Node3D", Icon::godot("Node3D"))
                .category("3D"),
        )
        .register_node(
            NodeType::new::<(Mesh3d, MeshMaterial3d<StandardMaterial>)>(
                "MeshInstance3D",
                Icon::godot("MeshInstance3D"),
            )
            .category("3D"),
        )
        .register_node(
            NodeType::from_fn("Camera3D", Icon::godot("Camera3D"), || {
                (
                    Camera3d::default(),
                    // Off while editing: the level shares the editor's world,
                    // and a live camera would draw over the editor itself.
                    // Godot's editor doesn't look through scene cameras either.
                    Camera {
                        is_active: false,
                        ..default()
                    },
                    Projection::default(),
                )
            })
            .category("3D"),
        )
        .register_node(
            NodeType::new::<DirectionalLight>(
                "DirectionalLight3D",
                Icon::godot("DirectionalLight3D"),
            )
            .category("Lights"),
        )
        .register_node(
            NodeType::new::<PointLight>("OmniLight3D", Icon::godot("OmniLight3D"))
                .category("Lights"),
        )
        .register_node(
            NodeType::new::<SpotLight>("SpotLight3D", Icon::godot("SpotLight3D"))
                .category("Lights"),
        );
    }
}

#[cfg(test)]
mod tests {
    use oddity_core::headless_app;
    use oddity_plugin_api::NodeTypes;

    use super::*;

    #[test]
    fn built_in_nodes_have_godots_icons_and_categories() {
        let mut app = headless_app();
        app.add_plugins(NodesPlugin);
        let types = app.world().resource::<NodeTypes>();
        let grouped: Vec<(&str, Vec<&str>)> = types
            .by_category()
            .into_iter()
            .map(|(category, types)| (category, types.iter().map(|t| t.name.as_ref()).collect()))
            .collect();
        assert_eq!(
            grouped,
            [
                ("Basic", vec!["Node"]),
                ("3D", vec!["Node3D", "MeshInstance3D", "Camera3D"]),
                (
                    "Lights",
                    vec!["DirectionalLight3D", "OmniLight3D", "SpotLight3D"]
                ),
            ]
        );
        for node_type in types.iter() {
            assert!(node_type.icon.exists(), "{} has no icon", node_type.name);
        }
    }
}
