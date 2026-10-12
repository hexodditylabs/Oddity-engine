//! The editor window's layout, after Godot 4: menu and main-screen bar on top,
//! Scene dock left, viewport centre, Inspector right, FileSystem/Output bottom.
//!
//! Phase 0 only sketches the layout to prove the theme, fonts and icons work.
//! Each dock becomes real in the phase that builds it.

use bevy::{
    feathers::{
        containers::{flex_spacer, pane, pane_body, pane_header},
        controls::{ButtonVariant, FeathersButton, FeathersToolButton},
        display::caption,
        theme::{ThemeBackgroundColor, ThemeTextColor, ThemedText},
        tokens,
    },
    prelude::*,
    text::{FontSourceTemplate, LetterSpacing},
    ui_widgets::Activate,
};
use oddity_icons::{Icon, IconSet, NodeColor, UiIcon};
use oddity_widgets::{brand, fonts::paths};

/// Godot's editor icon size.
const ICON: f32 = 16.0;

pub fn spawn(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn_scene(root());
}

fn root() -> impl Scene {
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            padding: px(4),
            row_gap: px(4),
        }
        ThemeBackgroundColor(tokens::WINDOW_BG)
        Children [
            @top_bar()
            --
            Node {
                flex_grow: 1.0,
                flex_direction: FlexDirection::Row,
                column_gap: px(4),
                min_height: px(0),
            }
            Children [
                @dock("Scene", Icon::godot("PackedScene"), "Scene tree arrives in Phase 3.", 260.0)
                --
                @viewport()
                --
                @dock("Inspector", Icon::godot("Tools"), "Inspector arrives in Phase 3.", 300.0)
            ]
            --
            @bottom_panel()
        ]
    }
}

/// Menus left, main screens centre, play controls right.
fn top_bar() -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(2),
            padding: UiRect::horizontal(px(6)),
            min_height: px(34),
        }
        Children [
            UiIcon { icon: Icon::oddity_cat(), size: 20.0 }
            Node { margin: UiRect::right(px(8)) }
            --
            @menu("Scene") -- @menu("Project") -- @menu("Debug") -- @menu("Editor") -- @menu("Help")
            --
            @flex_spacer()
            --
            @main_screen("2D", "2D") -- @main_screen("3D", "3D") -- @main_screen("Script", "Script") -- @main_screen("Game", "Game")
            --
            @flex_spacer()
            --
            @tool_icon("MainPlay") -- @tool_icon("Pause") -- @tool_icon("Stop") -- @tool_icon("PlayScene") -- @tool_icon("PlayCustom")
        ]
    }
}

fn menu(name: &'static str) -> impl Scene {
    bsn! {
        @FeathersButton {
            @caption: bsn! { @caption(name) },
            @variant: ButtonVariant::Plain,
        }
        on(move |_: On<Activate>| info!("{name} menu: not built yet"))
    }
}

fn main_screen(label: &'static str, icon: &'static str) -> impl Scene {
    bsn! {
        @FeathersButton {
            @caption: bsn_list! {
                UiIcon { icon: Icon::godot(icon), size: ICON }
                Node { margin: UiRect::right(px(4)) }
                --
                @caption(label)
            },
            @variant: ButtonVariant::Plain,
        }
        on(move |_: On<Activate>| info!("{label} screen: not built yet"))
    }
}

fn tool_icon(icon: &'static str) -> impl Scene {
    bsn! {
        @FeathersToolButton {
            @caption: bsn! { UiIcon { icon: Icon::godot(icon), size: ICON } },
            @variant: ButtonVariant::Plain,
        }
        on(move |_: On<Activate>| info!("{icon}: not built yet"))
    }
}

/// A side dock with a header and placeholder body.
fn dock(title: &'static str, icon: Icon, note: &'static str, width: f32) -> impl Scene {
    bsn! {
        @pane()
        Node { width: px(width) }
        Children [
            @pane_header()
            Children [
                Node { column_gap: px(6), align_items: AlignItems::Center } ThemedText
                Children [
                    UiIcon { icon: {icon.clone()}, size: ICON }
                    --
                    @caption(title)
                ]
            ]
            --
            @pane_body()
            Node { flex_grow: 1.0 }
            Children [ @dim_text(note) ]
        ]
    }
}

/// Centre area: where the 2D/3D viewport will live. For now, the welcome card.
fn viewport() -> impl Scene {
    bsn! {
        @pane()
        Node { flex_grow: 1.0 }
        Children [
            @pane_header()
            Children [
                Node { column_gap: px(6), align_items: AlignItems::Center } ThemedText
                Children [
                    UiIcon { icon: Icon::godot("3D"), size: ICON }
                    --
                    @caption("Viewport")
                ]
            ]
            --
            @pane_body()
            Node {
                flex_grow: 1.0,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(18),
            }
            BackgroundColor(brand::INK)
            Children [
                @welcome()
            ]
        ]
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

fn bottom_panel() -> impl Scene {
    let status = format!(
        "Oddity Engine {}  ·  Phase 0: workspace, theme, fonts and icons are up.",
        env!("CARGO_PKG_VERSION")
    );
    bsn! {
        @pane()
        Node { height: px(150) }
        Children [
            @pane_header()
            Children [
                Node { column_gap: px(14), align_items: AlignItems::Center } ThemedText
                Children [
                    Node { column_gap: px(6), align_items: AlignItems::Center } ThemedText
                    Children [
                        UiIcon { icon: Icon::godot("Folder"), size: ICON }
                        -- @caption("FileSystem")
                    ]
                    --
                    Node { column_gap: px(6), align_items: AlignItems::Center } ThemedText
                    Children [
                        UiIcon { icon: Icon::godot("Script"), size: ICON }
                        -- @caption("Output")
                    ]
                ]
            ]
            --
            @pane_body()
            Node { flex_grow: 1.0 }
            Children [
                Text(status)
                TextFont {
                    font: FontSourceTemplate::Handle(paths::MONO),
                    font_size: FontSize::Px(12.0),
                }
                ThemeTextColor(tokens::TEXT_MAIN)
            ]
        ]
    }
}

fn dim_text(text: &'static str) -> impl Scene {
    bsn! {
        Text(text)
        TextFont {
            font: FontSourceTemplate::Handle(paths::UI),
            font_size: FontSize::Px(13.0),
        }
        ThemeTextColor(tokens::TEXT_DIM)
    }
}
