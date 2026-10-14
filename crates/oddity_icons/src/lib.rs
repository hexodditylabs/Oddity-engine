//! Icons for Oddity Engine.
//!
//! Three icon sets are embedded in the binary as SVG and rendered on demand at
//! the exact pixel size needed, so they stay sharp at any UI scale:
//!
//! - [`IconSet::Godot`]: Godot's editor icons (MIT, see `third_party/godot`).
//!   Names match Godot's files, e.g. `"Node3D"`, `"Camera3D"`, `"Save"`.
//! - [`IconSet::At`]: the @icons pack by Voxybuns (MIT, see `third_party/at-icons`).
//!   Names are snake_case, e.g. `"hammer"`, `"shield"`. They are drawn in
//!   Godot's neutral colour and tinted per node type with [`NodeColor`].
//! - [`IconSet::Brand`]: Oddity Engine and Hex0ddity Labs marks from `branding/`.
//! - [`IconSet::Ui`]: Oddity's own small UI icons (title bar buttons) from
//!   `crates/oddity_icons/ui/`, for things Godot's set does not cover.
//!
//! In UI, add a [`UiIcon`] component and the [`IconsPlugin`] does the rest:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use oddity_icons::{Icon, NodeColor, UiIcon};
//!
//! fn spawn(mut commands: Commands) {
//!     commands.spawn(UiIcon::new(Icon::godot("Node3D"), 16.0));
//!     commands.spawn(UiIcon::new(Icon::at("hammer").tinted(NodeColor::Node2D), 16.0));
//!     commands.spawn(UiIcon::new(Icon::oddity_cat(), 64.0));
//! }
//! ```

mod plugin;
mod raster;

use std::borrow::Cow;

use bevy::color::{Color, Srgba};

pub use plugin::{IconCache, IconsPlugin, UiIcon};
pub use raster::{IconError, RgbaImage, rasterize};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/icons.rs"));
}

/// The colour Godot and @icons use for neutral icon strokes and fills.
/// Tinting replaces this colour.
pub const NEUTRAL_HEX: &str = "#e0e0e0";

/// Which embedded icon set an [`Icon`] comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IconSet {
    /// Godot's editor icons, by Godot's file name (`"Node3D"`).
    Godot,
    /// The @icons pack, by file name (`"hammer"`).
    At,
    /// Oddity Engine branding, by file name (`"oddity-cat"`).
    Brand,
    /// Oddity's own UI icons, by file name (`"window-close"`).
    Ui,
}

impl IconSet {
    /// Every set, in a stable order.
    pub const ALL: [IconSet; 4] = [IconSet::Godot, IconSet::At, IconSet::Brand, IconSet::Ui];

    fn table(self) -> &'static [(&'static str, &'static str)] {
        match self {
            IconSet::Godot => embedded::GODOT,
            IconSet::At => embedded::AT,
            IconSet::Brand => embedded::BRAND,
            IconSet::Ui => embedded::UI,
        }
    }

    /// Names of every icon in this set, sorted.
    pub fn names(self) -> impl Iterator<Item = &'static str> {
        self.table().iter().map(|(name, _)| *name)
    }

    /// Number of icons in this set.
    pub fn len(self) -> usize {
        self.table().len()
    }

    /// Whether the set is empty.
    pub fn is_empty(self) -> bool {
        self.table().is_empty()
    }

    /// The original SVG source of an icon, if it exists.
    pub fn svg(self, name: &str) -> Option<&'static str> {
        let table = self.table();
        table
            .binary_search_by(|(n, _)| (*n).cmp(name))
            .ok()
            .map(|i| table[i].1)
    }
}

/// Godot's icon colours per node family, as used by Godot and @icons.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NodeColor {
    /// Plain nodes (light grey).
    #[default]
    Node,
    /// 2D nodes (blue).
    Node2D,
    /// 3D nodes (red).
    Node3D,
    /// UI / Control nodes (green).
    Control,
    /// Meshes and resources (yellow).
    Mesh,
    /// Animation (purple).
    Animation,
    /// Folders in the FileSystem dock (Godot's folder blue).
    Folder,
}

impl NodeColor {
    /// Every node colour, in a stable order.
    pub const ALL: [NodeColor; 7] = [
        NodeColor::Node,
        NodeColor::Node2D,
        NodeColor::Node3D,
        NodeColor::Control,
        NodeColor::Mesh,
        NodeColor::Animation,
        NodeColor::Folder,
    ];

    /// The colour as a lowercase `#rrggbb` string.
    pub const fn hex(self) -> &'static str {
        match self {
            NodeColor::Node => NEUTRAL_HEX,
            NodeColor::Node2D => "#8da5f3",
            NodeColor::Node3D => "#fc7f7f",
            NodeColor::Control => "#8eef97",
            NodeColor::Mesh => "#ffca5f",
            NodeColor::Animation => "#c38ef1",
            // Measured from Godot 4.7's FileSystem dock.
            NodeColor::Folder => "#78a4e0",
        }
    }

    /// The colour as a Bevy [`Color`].
    pub fn color(self) -> Color {
        Srgba::hex(self.hex())
            .map(Color::from)
            .unwrap_or(Color::WHITE)
    }
}

/// A reference to one embedded icon, optionally recoloured.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Icon {
    set: IconSet,
    name: Cow<'static, str>,
    tint: Option<NodeColor>,
}

impl Default for Icon {
    /// Godot's plain `Node` icon.
    fn default() -> Self {
        Self::godot("Node")
    }
}

impl Icon {
    /// An icon from a given set.
    pub fn new(set: IconSet, name: impl Into<Cow<'static, str>>) -> Self {
        Self {
            set,
            name: name.into(),
            tint: None,
        }
    }

    /// A Godot editor icon, e.g. `Icon::godot("Node3D")`.
    pub fn godot(name: impl Into<Cow<'static, str>>) -> Self {
        Self::new(IconSet::Godot, name)
    }

    /// An @icons icon, e.g. `Icon::at("hammer")`.
    pub fn at(name: impl Into<Cow<'static, str>>) -> Self {
        Self::new(IconSet::At, name)
    }

    /// A brand icon, e.g. `Icon::brand("oddity-cat")`.
    pub fn brand(name: impl Into<Cow<'static, str>>) -> Self {
        Self::new(IconSet::Brand, name)
    }

    /// One of Oddity's own UI icons, e.g. `Icon::ui("window-close")`.
    pub fn ui(name: impl Into<Cow<'static, str>>) -> Self {
        Self::new(IconSet::Ui, name)
    }

    /// The 15x15 wizard cat, for sizes below 128 px.
    pub fn oddity_cat() -> Self {
        Self::brand("oddity-cat")
    }

    /// The striped 16x16 wizard cat, for 128 px and up.
    pub fn oddity_cat_striped() -> Self {
        Self::brand("oddity-cat-striped")
    }

    /// The ODDITY wordmark.
    pub fn oddity_wordmark() -> Self {
        Self::brand("oddity-wordmark")
    }

    /// The Hex0ddity Labs mark.
    pub fn hex0ddity_mark() -> Self {
        Self::brand("hex0ddity-mark")
    }

    /// Recolour the icon's neutral parts to a node family colour, like Godot does.
    pub fn tinted(mut self, color: NodeColor) -> Self {
        self.tint = (color != NodeColor::Node).then_some(color);
        self
    }

    /// Which set the icon comes from.
    pub fn set(&self) -> IconSet {
        self.set
    }

    /// The icon's name within its set.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The tint, if any.
    pub fn tint(&self) -> Option<NodeColor> {
        self.tint
    }

    /// Whether this icon exists in its set.
    pub fn exists(&self) -> bool {
        self.set.svg(&self.name).is_some()
    }

    /// The SVG source with the tint applied, or `None` if the icon does not exist.
    pub fn svg(&self) -> Option<Cow<'static, str>> {
        let source = self.set.svg(&self.name)?;
        Some(match self.tint {
            None => Cow::Borrowed(source),
            Some(color) => Cow::Owned(replace_ignore_case(source, NEUTRAL_HEX, color.hex())),
        })
    }

    /// Render the icon so its longest side is `size_px` pixels.
    pub fn rasterize(&self, size_px: u32) -> Result<RgbaImage, IconError> {
        let svg = self
            .svg()
            .ok_or_else(|| IconError::NotFound(self.to_string()))?;
        rasterize(&svg, size_px)
    }
}

impl std::fmt::Display for Icon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let set = match self.set {
            IconSet::Godot => "godot",
            IconSet::At => "at",
            IconSet::Brand => "brand",
            IconSet::Ui => "ui",
        };
        write!(f, "{set}:{}", self.name)?;
        if let Some(tint) = self.tint {
            write!(f, " ({tint:?})")?;
        }
        Ok(())
    }
}

fn replace_ignore_case(haystack: &str, needle: &str, with: &str) -> String {
    let lower = haystack.to_ascii_lowercase();
    let mut out = String::with_capacity(haystack.len());
    let mut last = 0;
    for (i, _) in lower.match_indices(needle) {
        out.push_str(&haystack[last..i]);
        out.push_str(with);
        last = i + needle.len();
    }
    out.push_str(&haystack[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_are_populated_and_sorted() {
        assert!(IconSet::Godot.len() > 1000, "Godot icons missing");
        assert!(IconSet::At.len() > 600, "@icons missing");
        for set in IconSet::ALL {
            let names: Vec<_> = set.names().collect();
            assert!(names.windows(2).all(|w| w[0] < w[1]), "{set:?} not sorted");
        }
    }

    #[test]
    fn known_icons_resolve() {
        assert!(Icon::godot("Node3D").exists());
        assert!(Icon::godot("Camera3D").exists());
        assert!(Icon::at("hammer").exists());
        assert!(Icon::oddity_cat().exists());
        assert!(Icon::oddity_cat_striped().exists());
        assert!(Icon::oddity_wordmark().exists());
        assert!(Icon::hex0ddity_mark().exists());
        for name in [
            "window-minimize",
            "window-maximize",
            "window-restore",
            "window-close",
        ] {
            assert!(Icon::ui(name).exists(), "missing ui icon {name}");
        }
        assert!(!Icon::godot("DefinitelyNotAnIcon").exists());
    }

    #[test]
    fn tint_replaces_neutral_colour() {
        let svg = Icon::at("hammer").tinted(NodeColor::Node3D).svg().unwrap();
        assert!(svg.contains(NodeColor::Node3D.hex()));
        assert!(!svg.to_ascii_lowercase().contains(NEUTRAL_HEX));
        // Tinting with the neutral colour is a no-op.
        assert_eq!(Icon::at("hammer").tinted(NodeColor::Node).tint(), None);
    }

    #[test]
    fn every_icon_renders() {
        for set in IconSet::ALL {
            for name in set.names() {
                let image = Icon::new(set, name)
                    .rasterize(16)
                    .unwrap_or_else(|e| panic!("{set:?}:{name} failed: {e}"));
                assert!(image.width <= 16 && image.height <= 16);
                assert_eq!(image.data.len(), (image.width * image.height * 4) as usize);
            }
        }
    }

    #[test]
    fn brand_cat_keeps_its_colours() {
        let image = Icon::oddity_cat().rasterize(15).unwrap();
        assert_eq!((image.width, image.height), (15, 15));
        // Pixel (1, 8) is in the cat's coral face.
        let i = ((8 * 15 + 1) * 4) as usize;
        assert_eq!(&image.data[i..i + 4], &[0xFF, 0x5A, 0x5F, 0xFF]);
    }
}
