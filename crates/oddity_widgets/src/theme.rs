//! Godot's editor palette, generated the way Godot does it, fed into Feathers' theme.
//!
//! Godot derives its whole dark theme from three numbers: a base colour, an
//! accent colour and a contrast amount (see `editor/themes/theme_modern.cpp`
//! in Godot 4.7). [`ThemeConfig::palette`] ports those rules, and
//! [`ThemeConfig::theme_props`] maps the result onto Feathers' semantic
//! tokens, so every Feathers widget picks it up.

use bevy::{
    color::{Alpha, Color, Hsva, Srgba},
    feathers::{
        dark_theme::create_dark_theme,
        theme::{SurfaceLevel, ThemeProps},
        tokens::semantic,
    },
    platform::collections::HashMap,
};

use crate::brand;

/// Godot's default contrast for dark themes.
pub const DEFAULT_CONTRAST: f32 = 0.3;

/// The inputs Godot builds an editor theme from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThemeConfig {
    /// Background tone of docks and panels.
    pub base: Color,
    /// Selection, focus and primary-button colour.
    pub accent: Color,
    /// How far darker and lighter surfaces move away from `base`.
    pub contrast: f32,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self::oddity()
    }
}

impl ThemeConfig {
    /// Godot's default dark grey with the Oddity hot-pink accent.
    pub fn oddity() -> Self {
        Self {
            base: Color::srgb(0.161, 0.161, 0.161),
            accent: brand::HOT_PINK,
            contrast: DEFAULT_CONTRAST,
        }
    }

    /// Godot 4's stock "Default" preset, blue accent included.
    pub fn godot() -> Self {
        Self {
            base: Color::srgb(0.161, 0.161, 0.161),
            accent: Color::srgb(0.337, 0.62, 1.0),
            contrast: DEFAULT_CONTRAST,
        }
    }

    /// Every derived colour, following Godot's rules.
    pub fn palette(&self) -> Palette {
        let base = self.base.to_srgba();
        let accent = self.accent.to_srgba();
        let white = Srgba::WHITE;
        let black = Srgba::BLACK;
        let c = self.contrast;
        let shade = |dimness: f32, saturation: f32| base_shade(base, c, dimness, saturation);

        Palette {
            base: base.into(),
            accent: accent.into(),

            dark_1: lerp(base, black, c * 1.15).into(),
            dark_3: shade(0.8, 0.9),
            contrast_1: lerp(base, white, (c * 1.15).max(DEFAULT_CONTRAST * 1.15)).into(),
            contrast_2: lerp(base, white, (c * 1.725).max(DEFAULT_CONTRAST * 1.725)).into(),

            surface_popup: shade(1.9, 0.9),
            surface_lowest: shade(1.7, 0.9),
            surface_lower: shade(1.1, 0.9),
            surface_low: shade(0.8, 1.0),
            surface_base: shade(0.0, 1.0),
            surface_high: shade(-1.3, 0.8),
            surface_higher: shade(-1.5, 0.8),
            surface_highest: shade(-2.2, 0.6),

            button_normal: shade(-2.0, 0.85),
            button_hover: shade(-2.9, 0.75),
            button_pressed: shade(-3.2, 0.75),
            button_disabled: shade(-1.4, 0.75),
            button_border: shade(-2.5, 0.75),

            flat_hover: shade(-1.2, 0.75),
            flat_pressed: shade(-2.0, 0.75),

            // Measured from Godot 4.7: #494949 selected tree rows, #323232
            // inspector categories, guide lines at 40% white.
            item_selected: shade(-2.5, 0.75),
            category: shade(-0.7, 0.9),
            relationship_line: white.with_alpha(0.4).into(),

            font: white.with_alpha(0.75).into(),
            font_secondary: white.with_alpha(0.55).into(),
            font_hover: white.with_alpha(0.85).into(),
            font_focus: white.into(),
            font_disabled: white.with_alpha(0.35).into(),

            highlight: accent.with_alpha(0.275).into(),
            selection: accent.with_alpha(0.4).into(),
            focus_border: accent.with_alpha(0.8).into(),

            success: Color::srgb(0.45, 0.95, 0.5),
            warning: Color::srgb(0.83, 0.78, 0.62),
            error: Color::srgb(1.0, 0.47, 0.42),

            axis_x: Color::srgb(0.96, 0.20, 0.32),
            axis_y: Color::srgb(0.53, 0.84, 0.01),
            axis_z: Color::srgb(0.16, 0.55, 0.96),
        }
    }

    /// The palette mapped onto Feathers' semantic tokens.
    pub fn theme_props(&self) -> ThemeProps {
        let p = self.palette();
        let accent = p.accent.to_srgba();
        let accent_hover: Color = lerp(accent, Srgba::WHITE, 0.1).into();
        let accent_pressed: Color = lerp(accent, Srgba::WHITE, 0.2).into();

        // Start from Feathers' dark theme for its token-to-semantic wiring,
        // then replace every colour.
        let mut props = create_dark_theme();
        props.semantic_base = HashMap::from([
            (semantic::SURFACE_WINDOW, p.surface_lowest),
            (semantic::SURFACE_PANE_BODY, p.surface_base),
            (semantic::SURFACE_PANE_HEADER, p.surface_low),
            (semantic::SURFACE_SUBPANE_BODY, p.surface_base),
            (semantic::SURFACE_SUBPANE_HEADER, p.surface_high),
            (semantic::SURFACE_GROUP, p.surface_high),
            (semantic::SURFACE_DIALOG, p.surface_popup),
            (semantic::SURFACE_MENU, p.surface_popup),
            (semantic::BORDER_DEFAULT, p.button_border),
            (semantic::TEXT_DEFAULT, p.font),
            (semantic::TEXT_DIM, p.font_secondary),
            (semantic::TEXT_DISABLED, p.font_disabled),
            (semantic::TEXT_ON_ACCENT, Color::WHITE),
            (semantic::FILL_ACCENT_DEFAULT, p.accent),
            (semantic::FILL_ACCENT_HOVER, accent_hover),
            (semantic::FILL_ACCENT_PRESSED, accent_pressed),
            (semantic::FILL_ACCENT_DISABLED, p.button_disabled),
            (semantic::FILL_SOLID_DEFAULT, p.button_normal),
            (semantic::FILL_SOLID_HOVER, p.button_hover),
            (semantic::FILL_SOLID_PRESSED, p.button_pressed),
            (semantic::FILL_SOLID_DISABLED, p.button_disabled),
            (semantic::FILL_ITEM_DEFAULT, Color::NONE),
            (semantic::FILL_ITEM_HOVER, p.flat_hover),
            (semantic::FILL_ITEM_PRESSED, p.flat_pressed),
            (semantic::FILL_FIELD_DEFAULT, p.dark_1),
            (semantic::FILL_FIELD_HOVER, p.dark_3),
            (semantic::FILL_FIELD_PRESSED, p.dark_3),
            (semantic::FILL_NONE, Color::NONE),
            (semantic::AXIS_X, p.axis_x),
            (semantic::AXIS_Y, p.axis_y),
            (semantic::AXIS_Z, p.axis_z),
            (semantic::AXIS_W, p.font_focus),
            (semantic::FOCUS_RING, p.focus_border),
        ]);
        // Raised surfaces get slightly brighter controls so they still stand out.
        let raised = HashMap::from([
            (semantic::FILL_SOLID_DEFAULT, p.button_hover),
            (semantic::FILL_SOLID_HOVER, p.button_pressed),
            (semantic::FILL_FIELD_DEFAULT, p.surface_base),
            (semantic::FILL_ITEM_HOVER, p.button_normal),
            (semantic::FILL_ITEM_PRESSED, p.button_hover),
        ]);
        props.semantic_overrides = HashMap::from([
            (SurfaceLevel::Highest, raised.clone()),
            (SurfaceLevel::Floating, raised),
        ]);
        props
    }
}

/// Every colour the editor uses, derived from a [`ThemeConfig`].
///
/// Names follow Godot's `ThemeConfiguration` fields.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(missing_docs)]
pub struct Palette {
    pub base: Color,
    pub accent: Color,
    pub dark_1: Color,
    pub dark_3: Color,
    pub contrast_1: Color,
    pub contrast_2: Color,
    pub surface_popup: Color,
    pub surface_lowest: Color,
    pub surface_lower: Color,
    pub surface_low: Color,
    pub surface_base: Color,
    pub surface_high: Color,
    pub surface_higher: Color,
    pub surface_highest: Color,
    pub button_normal: Color,
    pub button_hover: Color,
    pub button_pressed: Color,
    pub button_disabled: Color,
    pub button_border: Color,
    pub flat_hover: Color,
    pub flat_pressed: Color,
    pub item_selected: Color,
    pub category: Color,
    pub relationship_line: Color,
    pub font: Color,
    pub font_secondary: Color,
    pub font_hover: Color,
    pub font_focus: Color,
    pub font_disabled: Color,
    pub highlight: Color,
    pub selection: Color,
    pub focus_border: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub axis_x: Color,
    pub axis_y: Color,
    pub axis_z: Color,
}

/// Godot's `_get_base_color`: darken (positive) or lighten (negative) the base
/// by changing its HSV value, and scale its saturation.
fn base_shade(base: Srgba, contrast: f32, dimness: f32, saturation: f32) -> Color {
    let contrast = if dimness < 0.0 {
        contrast.clamp(-0.1, 0.5)
    } else {
        contrast
    };
    let mut hsv = Hsva::from(base);
    let t = contrast * dimness;
    hsv.value = (hsv.value + (0.0 - hsv.value) * t).clamp(0.0, 1.0);
    hsv.saturation *= saturation;
    Srgba::from(hsv).into()
}

/// Godot's `Color::lerp`: straight interpolation of sRGB components.
fn lerp(a: Srgba, b: Srgba, t: f32) -> Srgba {
    Srgba::new(
        a.red + (b.red - a.red) * t,
        a.green + (b.green - a.green) * t,
        a.blue + (b.blue - a.blue) * t,
        a.alpha + (b.alpha - a.alpha) * t,
    )
}

#[cfg(test)]
mod tests {
    use bevy::color::Mix;

    use super::*;

    fn hex(color: Color) -> String {
        color.to_srgba().to_hex()
    }

    #[test]
    fn godot_default_surfaces() {
        let p = ThemeConfig::godot().palette();
        // Base is unchanged; lower surfaces are darker, higher ones lighter.
        assert_eq!(hex(p.surface_base), hex(Color::srgb(0.161, 0.161, 0.161)));
        let v = |c: Color| Hsva::from(c.to_srgba()).value;
        assert!(v(p.surface_lowest) < v(p.surface_low));
        assert!(v(p.surface_low) < v(p.surface_base));
        assert!(v(p.surface_base) < v(p.surface_high));
        assert!(v(p.button_normal) < v(p.button_hover));
    }

    /// Colours sampled from a Godot 4.7.2 editor screenshot (default theme).
    /// Each must be reproduced within 2 steps per channel.
    #[test]
    fn matches_godot_4_7_screenshot() {
        let p = ThemeConfig::godot().palette();
        let near = |name: &str, colour: Color, expected: u32| {
            let c = colour.to_srgba();
            let got = [c.red, c.green, c.blue].map(|v| (v * 255.0).round() as i32);
            let want = [expected >> 16, (expected >> 8) & 0xff, expected & 0xff].map(|v| v as i32);
            assert!(
                got.iter().zip(want).all(|(g, w)| (g - w).abs() <= 2),
                "{name}: got {got:?}, Godot has {want:?}"
            );
        };
        near("window", p.surface_lowest, 0x141414);
        near("panel", p.surface_base, 0x292929);
        near("inset panel", p.surface_low, 0x1f1f1f);
        near("field", p.dark_1, 0x1b1b1b);
        near("toggled button", p.button_normal, 0x424242);
        near("selected row", p.item_selected, 0x494949);
        near("inspector category", p.category, 0x323232);
        // Text is translucent white; Godot's #c9c9c9 is what it looks like on a panel.
        let on_panel = p
            .surface_base
            .mix(&Color::WHITE.with_alpha(1.0), p.font.alpha());
        near("text on a panel", on_panel, 0xc9c9c9);
    }

    #[test]
    fn accent_feeds_selection() {
        let p = ThemeConfig::oddity().palette();
        assert_eq!(
            p.selection.to_srgba().with_alpha(1.0),
            brand::HOT_PINK.to_srgba()
        );
        assert!((p.selection.alpha() - 0.4).abs() < 1e-6);
    }

    #[test]
    fn every_semantic_token_has_a_colour() {
        let props = ThemeConfig::oddity().theme_props();
        for semantic in props.token_assignments.values() {
            assert!(
                props.semantic_base.contains_key(semantic),
                "no colour for {semantic:?}"
            );
        }
    }
}
