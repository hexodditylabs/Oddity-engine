//! Bevy integration: the [`UiIcon`] component and an image cache.

use bevy::{
    asset::RenderAssetUsages,
    image::Image,
    platform::collections::HashMap,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    ui::ComputedUiRenderTargetInfo,
};

use crate::Icon;

/// Renders [`UiIcon`]s and keeps one image per icon and pixel size.
pub struct IconsPlugin;

impl Plugin for IconsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<IconCache>().add_systems(
            PostUpdate,
            update_ui_icons.before(bevy::ui::UiSystems::Prepare),
        );
    }
}

/// Shows an [`Icon`] in UI at a size in logical pixels.
///
/// The icon is rendered for the UI's scale factor, so it stays sharp on
/// high-DPI screens, and re-rendered if that scale changes.
#[derive(Component, Clone, Debug)]
#[require(Node, ImageNode)]
pub struct UiIcon {
    /// What to show.
    pub icon: Icon,
    /// Width and height of the longest side, in logical pixels.
    pub size: f32,
}

impl Default for UiIcon {
    /// Godot's `Node` icon at Godot's standard 16 px.
    fn default() -> Self {
        Self::new(Icon::default(), 16.0)
    }
}

impl UiIcon {
    /// An icon at a size in logical pixels. Godot's editor icons are 16.
    pub fn new(icon: Icon, size: f32) -> Self {
        Self { icon, size }
    }
}

/// The icon and physical size an entity was last rendered with.
#[derive(Component)]
struct Rendered(Icon, u32);

/// Rendered icon images, keyed by icon and physical pixel size.
#[derive(Resource, Default)]
pub struct IconCache {
    images: HashMap<(Icon, u32), (Handle<Image>, UVec2)>,
}

impl IconCache {
    /// The image for an icon at a physical pixel size, rendering it on first use.
    ///
    /// Returns `None` (and logs once) if the icon does not exist.
    pub fn get(
        &mut self,
        images: &mut Assets<Image>,
        icon: &Icon,
        size_px: u32,
    ) -> Option<Handle<Image>> {
        self.get_sized(images, icon, size_px)
            .map(|(handle, _)| handle)
    }

    /// Like [`IconCache::get`], also returning the image's size in pixels
    /// (icons such as the wordmark are not square).
    pub fn get_sized(
        &mut self,
        images: &mut Assets<Image>,
        icon: &Icon,
        size_px: u32,
    ) -> Option<(Handle<Image>, UVec2)> {
        let key = (icon.clone(), size_px);
        if let Some(entry) = self.images.get(&key) {
            return Some(entry.clone());
        }
        match icon.rasterize(size_px) {
            Ok(rgba) => {
                let size = UVec2::new(rgba.width, rgba.height);
                let image = Image::new(
                    Extent3d {
                        width: rgba.width,
                        height: rgba.height,
                        depth_or_array_layers: 1,
                    },
                    TextureDimension::D2,
                    rgba.data,
                    TextureFormat::Rgba8UnormSrgb,
                    RenderAssetUsages::RENDER_WORLD,
                );
                let handle = images.add(image);
                self.images.insert(key, (handle.clone(), size));
                Some((handle, size))
            }
            Err(error) => {
                warn_once!("Oddity icons: {error}");
                None
            }
        }
    }

    /// How many images are cached.
    pub fn len(&self) -> usize {
        self.images.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}

fn update_ui_icons(
    mut commands: Commands,
    mut cache: ResMut<IconCache>,
    mut images: ResMut<Assets<Image>>,
    mut icons: Query<
        (
            Entity,
            &UiIcon,
            &mut ImageNode,
            &mut Node,
            Option<&ComputedUiRenderTargetInfo>,
            Option<&Rendered>,
        ),
        Or<(Changed<UiIcon>, Changed<ComputedUiRenderTargetInfo>)>,
    >,
) {
    for (entity, ui_icon, mut image_node, mut node, target, rendered) in &mut icons {
        let scale = target.map_or(1.0, |t| t.scale_factor());
        let size_px = (ui_icon.size * scale).round().max(1.0) as u32;

        if rendered.is_some_and(|r| r.0 == ui_icon.icon && r.1 == size_px) {
            continue;
        }
        let Some((handle, pixels)) = cache.get_sized(&mut images, &ui_icon.icon, size_px) else {
            node.width = Val::Px(ui_icon.size);
            node.height = Val::Px(ui_icon.size);
            continue;
        };
        // Lay out at the image's own aspect ratio, in logical pixels.
        let logical = pixels.as_vec2() * (ui_icon.size / size_px as f32);
        node.width = Val::Px(logical.x);
        node.height = Val::Px(logical.y);
        image_node.image = handle;
        commands
            .entity(entity)
            .insert(Rendered(ui_icon.icon.clone(), size_px));
    }
}
