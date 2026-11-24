//! All Bevy UI screens.

pub mod focus;
pub mod hud;
pub mod menus;
pub mod crash_report;
pub mod custom_menu;
pub mod play_menu;
pub mod scroll;
pub mod settings_menu;
pub mod skin_menu;
pub mod map_builder;
pub mod mode_creator;
pub mod splash;

use bevy::prelude::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            focus::FocusPlugin,
            menus::MenusPlugin,
            settings_menu::SettingsMenuPlugin,
            hud::HudPlugin,
            play_menu::PlayMenuPlugin,
            custom_menu::CustomMenuPlugin,
            scroll::ScrollPlugin,
            crash_report::CrashReportPlugin,
            splash::SplashPlugin,
            skin_menu::SkinMenuPlugin,
            map_builder::MapBuilderPlugin,
            mode_creator::ModeCreatorPlugin,
        ));
    }
}
