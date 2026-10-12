//! Editor fonts: Inter (Godot's UI font), Space Grotesk (brand) and Geist Mono (code).
//!
//! All three are under the SIL Open Font License; see `assets/fonts/*-LICENSE.txt`.

use std::path::{Path, PathBuf};

use bevy::{asset::io::embedded::EmbeddedAssetRegistry, prelude::*};

/// Embedded-asset paths for every font.
pub mod paths {
    /// Inter Regular: body text, like Godot's editor.
    pub const UI: &str = "embedded://oddity_widgets/fonts/Inter-Regular.ttf";
    /// Inter SemiBold: emphasis and headings.
    pub const UI_BOLD: &str = "embedded://oddity_widgets/fonts/Inter-SemiBold.ttf";
    /// Space Grotesk Medium: brand moments (Hub, splash, about).
    pub const BRAND: &str = "embedded://oddity_widgets/fonts/SpaceGrotesk-Medium.ttf";
    /// Space Grotesk Bold: brand titles.
    pub const BRAND_BOLD: &str = "embedded://oddity_widgets/fonts/SpaceGrotesk-Bold.ttf";
    /// Geist Mono Regular: code, output, numbers.
    pub const MONO: &str = "embedded://oddity_widgets/fonts/GeistMono-Regular.ttf";
    /// Geist Mono Medium: labels in the brand style (ENGINE, cargo run).
    pub const MONO_MEDIUM: &str = "embedded://oddity_widgets/fonts/GeistMono-Medium.ttf";
}

/// Handles to every editor font, loaded at startup.
#[derive(Resource, Clone, Debug)]
#[allow(missing_docs)]
pub struct OddityFonts {
    pub ui: Handle<Font>,
    pub ui_bold: Handle<Font>,
    pub brand: Handle<Font>,
    pub brand_bold: Handle<Font>,
    pub mono: Handle<Font>,
    pub mono_medium: Handle<Font>,
}

impl FromWorld for OddityFonts {
    fn from_world(world: &mut World) -> Self {
        let assets = world.resource::<AssetServer>();
        Self {
            ui: assets.load(paths::UI),
            ui_bold: assets.load(paths::UI_BOLD),
            brand: assets.load(paths::BRAND),
            brand_bold: assets.load(paths::BRAND_BOLD),
            mono: assets.load(paths::MONO),
            mono_medium: assets.load(paths::MONO_MEDIUM),
        }
    }
}

macro_rules! font {
    ($file:literal) => {
        (
            $file,
            include_bytes!(concat!("../assets/fonts/", $file)) as &'static [u8],
        )
    };
}

const FONTS: &[(&str, &[u8])] = &[
    font!("Inter-Regular.ttf"),
    font!("Inter-SemiBold.ttf"),
    font!("SpaceGrotesk-Medium.ttf"),
    font!("SpaceGrotesk-Bold.ttf"),
    font!("GeistMono-Regular.ttf"),
    font!("GeistMono-Medium.ttf"),
];

/// Feathers loads its fonts from fixed embedded paths. Registering ours at
/// those paths swaps Fira Sans for Inter and Fira Mono for Geist Mono in
/// every Feathers widget, without forking Feathers.
const FEATHERS_OVERRIDES: &[(&str, &str)] = &[
    (
        "bevy_feathers/assets/fonts/FiraSans-Regular.ttf",
        "Inter-Regular.ttf",
    ),
    (
        "bevy_feathers/assets/fonts/FiraSans-Bold.ttf",
        "Inter-SemiBold.ttf",
    ),
    (
        "bevy_feathers/assets/fonts/FiraMono-Medium.ttf",
        "GeistMono-Medium.ttf",
    ),
];

pub(crate) fn register(app: &mut App) {
    let registry = app.world().resource::<EmbeddedAssetRegistry>();
    let bytes = |file: &str| {
        FONTS
            .iter()
            .find(|(name, _)| *name == file)
            .map(|(_, bytes)| *bytes)
            .expect("font listed in FONTS")
    };
    for (file, data) in FONTS {
        let path = format!("oddity_widgets/fonts/{file}");
        registry.insert_asset(PathBuf::from(&path), Path::new(&path), *data);
    }
    for (feathers_path, file) in FEATHERS_OVERRIDES {
        registry.insert_asset(
            PathBuf::from(feathers_path),
            Path::new(feathers_path),
            bytes(file),
        );
    }
}
