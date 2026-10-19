//! The game's input map, read from the `[input]` section of `oddity.toml`
//! (`roadmap/scripting.md`, "Input actions"). The game program inserts
//! it as a resource at start; scripts read it through `ctx.input()`.

use std::fmt;

use oddity_project::InputSection;
use oddity_script::{Axis2Binding, Binding, BindingError, InputMap};

/// A binding in `[input]` that names no key, button or stick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputMapError {
    /// The action or axis it belongs to.
    pub entry: String,
    /// What is wrong with it.
    pub error: BindingError,
}

impl fmt::Display for InputMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "input map entry {:?}: {}", self.entry, self.error)
    }
}

impl std::error::Error for InputMapError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// The input map an `oddity.toml`'s `[input]` section describes, with every
/// binding checked.
pub fn input_map(input: &InputSection) -> Result<InputMap, InputMapError> {
    let mut map = InputMap::default();
    for (name, bindings) in &input.actions {
        let bindings = bindings
            .iter()
            .map(|binding| binding.parse())
            .collect::<Result<Vec<Binding>, _>>()
            .map_err(|error| InputMapError {
                entry: name.clone(),
                error,
            })?;
        map.add_action(name.clone(), bindings);
    }
    for (name, entry) in &input.axes2 {
        let in_entry = |error| InputMapError {
            entry: name.clone(),
            error,
        };
        let binding = |text: &Option<String>| {
            text.as_deref()
                .map(str::parse::<Binding>)
                .transpose()
                .map_err(in_entry)
        };
        let axis = Axis2Binding {
            left: binding(&entry.left)?,
            right: binding(&entry.right)?,
            up: binding(&entry.up)?,
            down: binding(&entry.down)?,
            stick: entry
                .stick
                .as_deref()
                .map(str::parse)
                .transpose()
                .map_err(in_entry)?,
        };
        map.add_axis2(name.clone(), axis);
    }
    Ok(map)
}
