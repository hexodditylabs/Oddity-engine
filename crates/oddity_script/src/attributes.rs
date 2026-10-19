//! What `#[script]` records about fields, as reflect custom attributes, for
//! the level files and the Inspector to read:
//!
//! ```ignore
//! let info = registration.type_info().as_struct()?;
//! let saved = info.field("speed")?.has_attribute::<Export>();
//! ```

use bevy::reflect::Reflect;

/// The field is saved in levels and shown in the Inspector: it is `pub`, or
/// marked `#[show]`, and not `#[hide]`.
#[derive(Reflect, Clone, Copy, Debug, PartialEq)]
pub struct Export;

/// `#[range(min, max)]`: a slider, and values are clamped (Unity's `[Range]`).
#[derive(Reflect, Clone, Copy, Debug, PartialEq)]
pub struct Range {
    /// The lowest value.
    pub min: f64,
    /// The highest value.
    pub max: f64,
}

/// `#[min(n)]`: never below this (Unity's `[Min]`).
#[derive(Reflect, Clone, Copy, Debug, PartialEq)]
pub struct Min(pub f64);

/// `#[tooltip("...")]`, or the field's `///` doc comment: shown on hover.
#[derive(Reflect, Clone, Copy, Debug, PartialEq)]
pub struct Tooltip(pub &'static str);

/// `#[header("...")]`: a bold heading above the field.
#[derive(Reflect, Clone, Copy, Debug, PartialEq)]
pub struct Header(pub &'static str);

/// `#[multiline]`: a multi-line text box for a `String` (Unity's `[TextArea]`).
#[derive(Reflect, Clone, Copy, Debug, PartialEq)]
pub struct Multiline;
