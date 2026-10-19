//! Simple shapes a level can hold until it can refer to mesh assets
//! (Phase 4): a [`MeshShape`] is saved inline, the way Godot saves a
//! `BoxMesh` inside a scene, and becomes a mesh and a material when it
//! loads.

use bevy::{platform::collections::HashMap, prelude::*, reflect::std_traits::ReflectDefault};

/// A box, sphere, capsule, cylinder or plane in one colour, on a
/// MeshInstance3D node.
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
#[reflect(Component, Default)]
#[require(Transform, Visibility)]
pub struct MeshShape {
    /// The shape and its size.
    pub shape: Shape,
    /// Its colour.
    pub color: Color,
}

impl Default for MeshShape {
    fn default() -> Self {
        Self {
            shape: Shape::default(),
            color: Color::WHITE,
        }
    }
}

/// A [`MeshShape`]'s shape, in metres, centred on the node.
#[derive(Reflect, Clone, Debug, PartialEq)]
#[reflect(Default)]
pub enum Shape {
    /// A box this size.
    Box {
        /// Width, height and depth.
        size: Vec3,
    },
    /// A sphere.
    Sphere {
        /// Its radius.
        radius: f32,
    },
    /// An upright capsule.
    Capsule {
        /// Its radius.
        radius: f32,
        /// Its height, end to end.
        height: f32,
    },
    /// An upright cylinder.
    Cylinder {
        /// Its radius.
        radius: f32,
        /// Its height.
        height: f32,
    },
    /// A flat plane facing up.
    Plane {
        /// Width and depth.
        size: Vec2,
    },
}

impl Default for Shape {
    fn default() -> Self {
        Shape::Box { size: Vec3::ONE }
    }
}

impl Shape {
    fn mesh(&self) -> Mesh {
        match *self {
            Shape::Box { size } => Cuboid::from_size(size).into(),
            Shape::Sphere { radius } => Sphere::new(radius).into(),
            Shape::Capsule { radius, height } => {
                Capsule3d::new(radius, (height - 2.0 * radius).max(0.0)).into()
            }
            Shape::Cylinder { radius, height } => Cylinder::new(radius, height).into(),
            Shape::Plane { size } => Plane3d::new(Vec3::Y, size / 2.0).into(),
        }
    }
}

/// Meshes and materials already made, shared by every shape that looks the
/// same: a hundred bullets are one mesh and one material on the GPU, as a
/// Godot resource is shared by the scenes that use it.
#[derive(Default)]
pub(crate) struct Made {
    meshes: HashMap<String, Handle<Mesh>>,
    materials: HashMap<[u32; 4], Handle<StandardMaterial>>,
}

/// Give new and changed shapes their mesh and material. Without a renderer
/// (headless tests) there is nothing to do.
pub(crate) fn build_shapes(
    mut commands: Commands,
    shapes: Query<(Entity, &MeshShape), Changed<MeshShape>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
    mut made: Local<Made>,
) {
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    for (entity, shape) in &shapes {
        let mesh = made
            .meshes
            .entry(format!("{:?}", shape.shape))
            .or_insert_with(|| meshes.add(shape.shape.mesh()))
            .clone();
        let color = shape.color.to_srgba().to_f32_array().map(f32::to_bits);
        let material = made
            .materials
            .entry(color)
            .or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color: shape.color,
                    ..default()
                })
            })
            .clone();
        commands
            .entity(entity)
            .insert((Mesh3d(mesh), MeshMaterial3d(material)));
    }
}
