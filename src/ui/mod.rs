//! All Bevy UI screens.

pub mod focus;
pub mod hud;
pub mod menus;
pub mod custom_menu;
pub mod play_menu;
pub mod settings_menu;

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
        ));
    }
}
