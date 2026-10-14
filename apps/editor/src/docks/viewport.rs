//! The centre dock: the 3D/2D viewport with Godot's toolbar above it.
//!
//! The view itself arrives in Phase 3; until then it shows the welcome card
//! on Godot's default clear colour. The toolbar is real already: its state
//! lives in [`ViewportSettings`], which the Phase 3 gizmos read, and the same
//! keys as Godot drive it (Q, W, E, R for the tools, T for local space, Y for
//! snapping).

use bevy::{
    feathers::{
        controls::{ButtonVariant, FeathersButton},
        display::caption,
        theme::{ThemeBackgroundColor, ThemeTextColor},
        tokens,
    },
    prelude::*,
    text::{FontSourceTemplate, LetterSpacing},
    ui_widgets::Activate,
};
use oddity_icons::{Icon, IconSet, NodeColor, UiIcon};
use oddity_widgets::{
    brand,
    button::{Toggled, tool_button},
    dock::Dock,
    fonts::paths,
};

/// Godot's editor icon size.
const ICON: f32 = 16.0;
/// Godot's default viewport clear colour, `Color(0.3, 0.3, 0.3)`.
const CLEAR_COLOUR: Color = Color::srgb(0.3, 0.3, 0.3);

pub struct ViewportPlugin;

impl Plugin for ViewportPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ViewportSettings>()
            .add_systems(Update, (follow_mode_tabs, show_settings).chain());
    }
}

/// How the viewport edits, chosen in its toolbar.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewportSettings {
    /// 3D or 2D, from the centre dock's tabs.
    pub mode: ViewMode,
    /// What dragging in the view does.
    pub tool: Tool,
    /// Move, rotate and scale along the node's own axes instead of the world's.
    pub local_space: bool,
    /// Snap moves, rotations and scales to fixed steps.
    pub snap: bool,
}

/// Which view the centre dock shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewMode {
    /// The 3D view.
    #[default]
    ThreeD,
    /// The 2D view.
    TwoD,
}

/// Godot's transform tools.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    /// Click to select (Q).
    #[default]
    Select,
    /// Drag to move (W).
    Move,
    /// Drag to rotate (E).
    Rotate,
    /// Drag to scale (R).
    Scale,
}

impl Tool {
    fn icon(self) -> Icon {
        Icon::godot(match self {
            Tool::Select => "ToolSelect",
            Tool::Move => "ToolMove",
            Tool::Rotate => "ToolRotate",
            Tool::Scale => "ToolScale",
        })
    }
}

/// What a toolbar button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewportAction {
    /// Pick a tool.
    Tool(Tool),
    /// Flip [`ViewportSettings::local_space`].
    ToggleLocalSpace,
    /// Flip [`ViewportSettings::snap`].
    ToggleSnap,
}

impl ViewportAction {
    /// Apply this action to the settings.
    pub fn apply(self, settings: &mut ViewportSettings) {
        match self {
            ViewportAction::Tool(tool) => settings.tool = tool,
            ViewportAction::ToggleLocalSpace => settings.local_space = !settings.local_space,
            ViewportAction::ToggleSnap => settings.snap = !settings.snap,
        }
    }

    /// Whether the button for this action shows as pressed.
    fn is_on(self, settings: &ViewportSettings) -> bool {
        match self {
            ViewportAction::Tool(tool) => settings.tool == tool,
            ViewportAction::ToggleLocalSpace => settings.local_space,
            ViewportAction::ToggleSnap => settings.snap,
        }
    }
}

/// A toolbar button and what it does.
#[derive(Component, Clone, Copy, Debug)]
struct ViewportButton(ViewportAction);

impl Default for ViewportButton {
    fn default() -> Self {
        Self(ViewportAction::Tool(Tool::Select))
    }
}

/// Marks the centre dock, whose tabs pick the [`ViewMode`].
#[derive(Component, Default, Clone)]
pub struct CentreDock;

/// The dock body: toolbar above the view.
pub fn viewport_page() -> impl Scene {
    bsn! {
        Node { flex_grow: 1.0, flex_direction: FlexDirection::Column, min_height: px(0) }
        Children [
            Node {
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(2),
                padding: UiRect::axes(px(6), px(6)),
            }
            Children [
                @toolbar_button(ViewportAction::Tool(Tool::Select), Tool::Select.icon())
                -- @toolbar_button(ViewportAction::Tool(Tool::Move), Tool::Move.icon())
                -- @toolbar_button(ViewportAction::Tool(Tool::Rotate), Tool::Rotate.icon())
                -- @toolbar_button(ViewportAction::Tool(Tool::Scale), Tool::Scale.icon())
                -- @separator()
                -- @toolbar_button(ViewportAction::ToggleLocalSpace, Icon::godot("Object"))
                -- @toolbar_button(ViewportAction::ToggleSnap, Icon::godot("Snap"))
            ]
            --
            Node {
                flex_grow: 1.0,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
            }
            BackgroundColor(CLEAR_COLOUR)
            Children [ @welcome() ]
        ]
    }
}

fn toolbar_button(action: ViewportAction, icon: Icon) -> impl Scene {
    bsn! {
        @tool_button(icon)
        ViewportButton({action})
        Toggled
        on(|activate: On<Activate>, buttons: Query<&ViewportButton>, mut settings: ResMut<ViewportSettings>| {
            if let Ok(ViewportButton(action)) = buttons.get(activate.entity) {
                action.apply(&mut settings);
            }
        })
    }
}

/// Godot's thin vertical line between toolbar groups.
fn separator() -> impl Scene {
    bsn! {
        Node { width: px(1), height: px(20), margin: UiRect::horizontal(px(6)) }
        ThemeBackgroundColor(tokens::BUTTON_BG)
    }
}

/// Mirror the centre dock's 3D/2D tabs into [`ViewportSettings::mode`].
fn follow_mode_tabs(
    docks: Query<&Dock, (With<CentreDock>, Changed<Dock>)>,
    mut settings: ResMut<ViewportSettings>,
) {
    for dock in &docks {
        let mode = if dock.active == 1 {
            ViewMode::TwoD
        } else {
            ViewMode::ThreeD
        };
        if settings.mode != mode {
            settings.mode = mode;
        }
    }
}

/// Press the buttons that match the settings.
fn show_settings(
    settings: Res<ViewportSettings>,
    added: Query<(), Added<ViewportButton>>,
    mut buttons: Query<(&ViewportButton, &mut Toggled)>,
) {
    if !settings.is_changed() && added.is_empty() {
        return;
    }
    for (ViewportButton(action), mut toggled) in &mut buttons {
        toggled.set_if_neq(Toggled(action.is_on(&settings)));
    }
}

fn welcome() -> impl Scene {
    let counts = format!(
        "{} Godot icons  ·  {} @icons  ·  Bevy 0.20",
        IconSet::Godot.len(),
        IconSet::At.len()
    );
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(18),
        }
        Children [
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(28),
            }
            Children [
                UiIcon { icon: Icon::oddity_cat_striped(), size: 128.0 }
                --
                Node { flex_direction: FlexDirection::Column, row_gap: px(12) }
                Children [
                    UiIcon { icon: Icon::oddity_wordmark(), size: 234.0 }
                    --
                    Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center }
                    Children [
                        Text("ENGINE")
                        TextFont {
                            font: FontSourceTemplate::Handle(paths::MONO_MEDIUM),
                            font_size: FontSize::Px(16.0),
                        }
                        LetterSpacing::Px(6.0)
                        TextColor(brand::HOT_PINK)
                        --
                        Text("cargo run ↵")
                        TextFont {
                            font: FontSourceTemplate::Handle(paths::MONO),
                            font_size: FontSize::Px(11.0),
                        }
                        LetterSpacing::Px(1.2)
                        ThemeTextColor(tokens::TEXT_DIM)
                    ]
                ]
            ]
            --
            @icon_row()
            --
            Text(counts)
            TextFont {
                font: FontSourceTemplate::Handle(paths::MONO),
                font_size: FontSize::Px(11.0),
            }
            ThemeTextColor(tokens::TEXT_DIM)
            --
            @FeathersButton {
                @caption: bsn! { @caption("New Project") },
                @variant: ButtonVariant::Primary,
            }
            on(|_: On<Activate>| info!("New Project: the Hub arrives in Phase 5"))
        ]
    }
}

/// One of each node family, from Godot's set and the @icons set.
fn icon_row() -> impl Scene {
    bsn! {
        Node { column_gap: px(10), align_items: AlignItems::Center }
        Children [
            UiIcon { icon: Icon::godot("Node"), size: ICON }
            -- UiIcon { icon: Icon::godot("Node2D"), size: ICON }
            -- UiIcon { icon: Icon::godot("Node3D"), size: ICON }
            -- UiIcon { icon: Icon::godot("Control"), size: ICON }
            -- UiIcon { icon: Icon::godot("Camera3D"), size: ICON }
            -- UiIcon { icon: Icon::godot("MeshInstance3D"), size: ICON }
            -- UiIcon { icon: Icon::godot("AnimationPlayer"), size: ICON }
            -- UiIcon { icon: Icon::godot("Script"), size: ICON }
            -- Node { width: px(1), height: px(16), margin: UiRect::horizontal(px(6)) }
               ThemeBackgroundColor(tokens::TEXT_DIM)
            -- UiIcon { icon: Icon::at("hammer"), size: ICON }
            -- UiIcon { icon: {Icon::at("hammer").tinted(NodeColor::Node2D)}, size: ICON }
            -- UiIcon { icon: {Icon::at("hammer").tinted(NodeColor::Node3D)}, size: ICON }
            -- UiIcon { icon: {Icon::at("hammer").tinted(NodeColor::Control)}, size: ICON }
            -- UiIcon { icon: {Icon::at("hammer").tinted(NodeColor::Mesh)}, size: ICON }
            -- UiIcon { icon: {Icon::at("hammer").tinted(NodeColor::Animation)}, size: ICON }
        ]
    }
}
