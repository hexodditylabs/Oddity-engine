//! Oddity Engine brand colours, from `branding/oddity-engine-brand.html`.

use bevy::color::{Color, Srgba};

/// Coral: the cat's face and the wordmark. `#FF5A5F`
pub const CORAL: Color = Color::Srgba(Srgba::rgb(1.0, 90.0 / 255.0, 95.0 / 255.0));
/// Hot pink: the accent, cursor block and hat band. `#FF2D55`
pub const HOT_PINK: Color = Color::Srgba(Srgba::rgb(1.0, 45.0 / 255.0, 85.0 / 255.0));
/// Pale pink: the hat and highlights. `#FFE4E1`
pub const PALE_PINK: Color = Color::Srgba(Srgba::rgb(1.0, 228.0 / 255.0, 225.0 / 255.0));
/// Pure black: logo backgrounds. `#000000`
pub const BLACK: Color = Color::Srgba(Srgba::rgb(0.0, 0.0, 0.0));
/// Ink: near-black panels. `#0A0A0A`
pub const INK: Color = Color::Srgba(Srgba::rgb(10.0 / 255.0, 10.0 / 255.0, 10.0 / 255.0));
