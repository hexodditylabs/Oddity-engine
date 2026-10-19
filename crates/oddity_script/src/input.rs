//! Input for scripts (`roadmap/scripting.md`, "Input actions"): raw keys and
//! mouse through `ctx.input()`, and named actions and axes from the
//! project's input map.
//!
//! ```ignore
//! let movement = ctx.input().axis2("move");      // WASD or left stick
//! if ctx.input().action_just_pressed("fire") { .. }
//! if ctx.input().pressed(KeyCode::Escape) { .. }
//! ```
//!
//! A binding is written the way `oddity.toml` writes it: a Bevy `KeyCode`
//! (`"Space"`, `"KeyW"`), `"Mouse"` and a `MouseButton` (`"MouseLeft"`), or
//! `"Gamepad"` and a `GamepadButton` (`"GamepadSouth"`). Gamepad buttons
//! count on every connected gamepad.

use std::{collections::HashMap, fmt, str::FromStr};

use bevy::{
    input::{
        gamepad::Gamepad,
        mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    },
    prelude::*,
    reflect::enums::{DynamicEnum, DynamicVariant, Enum},
    window::PrimaryWindow,
};

/// One key, mouse button or gamepad button an action listens to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Binding {
    /// A keyboard key, by its position on the keyboard.
    Key(KeyCode),
    /// A mouse button.
    Mouse(MouseButton),
    /// A button on any gamepad.
    Gamepad(GamepadButton),
}

/// The analog stick (or D-pad) that drives an [`Axis2Binding`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stick {
    /// `"LeftStick"`.
    Left,
    /// `"RightStick"`.
    Right,
    /// `"DPad"`.
    DPad,
}

/// A two-way axis made of four directions and, optionally, a stick:
/// `axis2("move")`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Axis2Binding {
    /// Pushes towards -x.
    pub left: Option<Binding>,
    /// Pushes towards +x.
    pub right: Option<Binding>,
    /// Pushes towards +y.
    pub up: Option<Binding>,
    /// Pushes towards -y.
    pub down: Option<Binding>,
    /// Adds the stick's position.
    pub stick: Option<Stick>,
}

/// Text that names no key, button or stick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingError {
    /// The text as written.
    pub binding: String,
}

impl fmt::Display for BindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown input binding {:?}: use a Bevy KeyCode (\"Space\", \"KeyW\"), \
             \"Mouse\" and a MouseButton (\"MouseLeft\"), \"Gamepad\" and a \
             GamepadButton (\"GamepadSouth\"), or for a stick \"LeftStick\", \
             \"RightStick\" or \"DPad\"",
            self.binding
        )
    }
}

impl std::error::Error for BindingError {}

const MOUSE: &str = "Mouse";
const GAMEPAD: &str = "Gamepad";

/// The unit variant of `T` called `name`. Reflection reads the names off
/// Bevy's own enums, so every key Bevy knows works without a table here.
fn unit_variant<T: FromReflect>(name: &str) -> Option<T> {
    T::from_reflect(&DynamicEnum::new(name, DynamicVariant::Unit))
}

impl FromStr for Binding {
    type Err = BindingError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let parsed = if let Some(button) = text.strip_prefix(MOUSE) {
            unit_variant(button).map(Binding::Mouse)
        } else if let Some(button) = text.strip_prefix(GAMEPAD) {
            unit_variant(button).map(Binding::Gamepad)
        } else {
            unit_variant(text).map(Binding::Key)
        };
        parsed.ok_or_else(|| BindingError {
            binding: text.to_owned(),
        })
    }
}

impl fmt::Display for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Binding::Key(key) => f.write_str(key.variant_name()),
            Binding::Mouse(button) => write!(f, "{MOUSE}{}", button.variant_name()),
            Binding::Gamepad(button) => write!(f, "{GAMEPAD}{}", button.variant_name()),
        }
    }
}

impl FromStr for Stick {
    type Err = BindingError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "LeftStick" => Ok(Stick::Left),
            "RightStick" => Ok(Stick::Right),
            "DPad" => Ok(Stick::DPad),
            _ => Err(BindingError {
                binding: text.to_owned(),
            }),
        }
    }
}

impl fmt::Display for Stick {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Stick::Left => "LeftStick",
            Stick::Right => "RightStick",
            Stick::DPad => "DPad",
        })
    }
}

/// The project's named actions and axes (`[input]` in `oddity.toml`).
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct InputMap {
    actions: HashMap<String, Vec<Binding>>,
    axes2: HashMap<String, Axis2Binding>,
}

impl InputMap {
    /// Add (or replace) the action `name`, pressed by any of `bindings`.
    pub fn add_action(&mut self, name: impl Into<String>, bindings: Vec<Binding>) -> &mut Self {
        self.actions.insert(name.into(), bindings);
        self
    }

    /// Add (or replace) the two-way axis `name`.
    pub fn add_axis2(&mut self, name: impl Into<String>, axis: Axis2Binding) -> &mut Self {
        self.axes2.insert(name.into(), axis);
        self
    }

    /// The bindings of the action `name`.
    pub fn action(&self, name: &str) -> Option<&[Binding]> {
        self.actions.get(name).map(Vec::as_slice)
    }

    /// The two-way axis `name`.
    pub fn axis2(&self, name: &str) -> Option<&Axis2Binding> {
        self.axes2.get(name)
    }
}

/// What the player is pressing, this frame: `ctx.input()`.
pub struct Input<'w> {
    world: &'w World,
}

impl<'w> Input<'w> {
    pub(crate) fn new(world: &'w World) -> Self {
        Self { world }
    }

    /// Whether `key` is held down.
    pub fn pressed(&self, key: KeyCode) -> bool {
        self.buttons::<KeyCode>()
            .is_some_and(|keys| keys.pressed(key))
    }

    /// Whether `key` went down this frame.
    pub fn just_pressed(&self, key: KeyCode) -> bool {
        self.buttons::<KeyCode>()
            .is_some_and(|keys| keys.just_pressed(key))
    }

    /// Whether `key` came up this frame.
    pub fn just_released(&self, key: KeyCode) -> bool {
        self.buttons::<KeyCode>()
            .is_some_and(|keys| keys.just_released(key))
    }

    /// Whether the mouse `button` is held down.
    pub fn mouse_pressed(&self, button: MouseButton) -> bool {
        self.buttons::<MouseButton>()
            .is_some_and(|buttons| buttons.pressed(button))
    }

    /// Whether the mouse `button` went down this frame.
    pub fn mouse_just_pressed(&self, button: MouseButton) -> bool {
        self.buttons::<MouseButton>()
            .is_some_and(|buttons| buttons.just_pressed(button))
    }

    /// Whether the mouse `button` came up this frame.
    pub fn mouse_just_released(&self, button: MouseButton) -> bool {
        self.buttons::<MouseButton>()
            .is_some_and(|buttons| buttons.just_released(button))
    }

    /// How far the mouse moved this frame, in raw device units: for
    /// turning a camera, not for finding the cursor.
    pub fn mouse_motion(&self) -> Vec2 {
        self.world
            .get_resource::<AccumulatedMouseMotion>()
            .map_or(Vec2::ZERO, |motion| motion.delta)
    }

    /// How far the wheel turned this frame: lines on most mice, pixels on
    /// touchpads.
    pub fn mouse_scroll(&self) -> Vec2 {
        self.world
            .get_resource::<AccumulatedMouseScroll>()
            .map_or(Vec2::ZERO, |scroll| scroll.delta)
    }

    /// Where the cursor is in the game window, in logical pixels from the
    /// top left; `None` when it is outside the window.
    pub fn cursor_position(&self) -> Option<Vec2> {
        let mut windows = self
            .world
            .try_query_filtered::<&Window, With<PrimaryWindow>>()?;
        windows.iter(self.world).next()?.cursor_position()
    }

    /// Whether any of the action's bindings is held down.
    ///
    /// # Panics
    ///
    /// If the input map has no action `name`: a misspelt name switches the
    /// script off with a message, instead of a button that silently never
    /// works.
    pub fn action_pressed(&self, name: &str) -> bool {
        self.action(name).iter().any(|b| self.binding_pressed(b))
    }

    /// Whether the action was pressed this frame: one of its bindings went
    /// down while none of the others was already held.
    ///
    /// # Panics
    ///
    /// If the input map has no action `name`.
    pub fn action_just_pressed(&self, name: &str) -> bool {
        let bindings = self.action(name);
        bindings.iter().any(|b| self.binding_just_pressed(b))
            && !bindings
                .iter()
                .any(|b| self.binding_pressed(b) && !self.binding_just_pressed(b))
    }

    /// Whether the action was let go this frame: one of its bindings came
    /// up and none is still held.
    ///
    /// # Panics
    ///
    /// If the input map has no action `name`.
    pub fn action_just_released(&self, name: &str) -> bool {
        let bindings = self.action(name);
        bindings.iter().any(|b| self.binding_just_released(b))
            && !bindings.iter().any(|b| self.binding_pressed(b))
    }

    /// How hard the action is pressed, from 0 to 1: analog for gamepad
    /// triggers, 0 or 1 for keys and buttons.
    ///
    /// # Panics
    ///
    /// If the input map has no action `name`.
    pub fn action_strength(&self, name: &str) -> f32 {
        self.action(name)
            .iter()
            .map(|b| self.binding_strength(b))
            .fold(0.0, f32::max)
    }

    /// From -1 to 1: the `positive` action's strength minus the `negative`
    /// one's. `axis("left", "right")` for steering.
    ///
    /// # Panics
    ///
    /// If the input map lacks either action.
    pub fn axis(&self, negative: &str, positive: &str) -> f32 {
        self.action_strength(positive) - self.action_strength(negative)
    }

    /// The two-way axis `name`, with +y up: its keys plus its stick, at
    /// most 1 long, so moving diagonally isn't faster.
    ///
    /// # Panics
    ///
    /// If the input map has no axis `name`.
    pub fn axis2(&self, name: &str) -> Vec2 {
        let Some(axis) = self.map().and_then(|map| map.axis2(name)) else {
            panic!("no input axis {name:?}: add it to [input.axes2] in oddity.toml");
        };
        let strength = |binding: &Option<Binding>| {
            binding
                .as_ref()
                .map_or(0.0, |binding| self.binding_strength(binding))
        };
        let keys = Vec2::new(
            strength(&axis.right) - strength(&axis.left),
            strength(&axis.up) - strength(&axis.down),
        );
        let stick = axis.stick.map_or(Vec2::ZERO, |stick| self.stick(stick));
        (keys + stick).clamp_length_max(1.0)
    }

    fn map(&self) -> Option<&'w InputMap> {
        self.world.get_resource::<InputMap>()
    }

    fn action(&self, name: &str) -> &'w [Binding] {
        match self.map().and_then(|map| map.action(name)) {
            Some(bindings) => bindings,
            None => panic!("no input action {name:?}: add it to [input.actions] in oddity.toml"),
        }
    }

    fn buttons<T: Copy + Eq + std::hash::Hash + Send + Sync + 'static>(
        &self,
    ) -> Option<&'w ButtonInput<T>> {
        self.world.get_resource::<ButtonInput<T>>()
    }

    /// Every connected gamepad.
    fn gamepads(&self) -> Vec<&'w Gamepad> {
        self.world
            .try_query::<&Gamepad>()
            .map(|mut gamepads| gamepads.iter(self.world).collect())
            .unwrap_or_default()
    }

    fn binding_pressed(&self, binding: &Binding) -> bool {
        match *binding {
            Binding::Key(key) => self.pressed(key),
            Binding::Mouse(button) => self.mouse_pressed(button),
            Binding::Gamepad(button) => self.gamepads().iter().any(|pad| pad.pressed(button)),
        }
    }

    fn binding_just_pressed(&self, binding: &Binding) -> bool {
        match *binding {
            Binding::Key(key) => self.just_pressed(key),
            Binding::Mouse(button) => self.mouse_just_pressed(button),
            Binding::Gamepad(button) => self.gamepads().iter().any(|pad| pad.just_pressed(button)),
        }
    }

    fn binding_just_released(&self, binding: &Binding) -> bool {
        match *binding {
            Binding::Key(key) => self.just_released(key),
            Binding::Mouse(button) => self.mouse_just_released(button),
            Binding::Gamepad(button) => self.gamepads().iter().any(|pad| pad.just_released(button)),
        }
    }

    fn binding_strength(&self, binding: &Binding) -> f32 {
        match *binding {
            Binding::Gamepad(button) => self
                .gamepads()
                .iter()
                .map(|pad| match pad.get(button) {
                    // A trigger reports how far it is in; a button set
                    // pressed without a value (by code) counts as all the
                    // way.
                    Some(value) if value > 0.0 => value,
                    _ if pad.pressed(button) => 1.0,
                    _ => 0.0,
                })
                .fold(0.0, f32::max),
            _ => {
                if self.binding_pressed(binding) {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }

    /// The stick pushed furthest, over every gamepad.
    fn stick(&self, stick: Stick) -> Vec2 {
        self.gamepads()
            .iter()
            .map(|pad| match stick {
                Stick::Left => pad.left_stick(),
                Stick::Right => pad.right_stick(),
                Stick::DPad => pad.dpad(),
            })
            .max_by(|a, b| a.length_squared().total_cmp(&b.length_squared()))
            .unwrap_or(Vec2::ZERO)
    }
}
