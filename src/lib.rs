//! HALLWAYS — first-person procedural horror-survival (Bevy).
//!
//! The crate is a library plus a thin binary so gameplay logic can be unit
//! tested with `cargo test` without opening a window.

pub mod audio;
pub mod capture;
pub mod cutscene;
pub mod hollow;
pub mod interaction;
pub mod music;
pub mod physics;
pub mod player;
pub mod rng;
pub mod run;
pub mod settings;
pub mod state;
pub mod theme;
pub mod ui;
pub mod world;

use bevy::prelude::*;

/// Everything game-specific. Add after `DefaultPlugins`.
pub struct HallwaysPlugin;

impl Plugin for HallwaysPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<rng::RunSeed>().add_plugins((
            settings::SettingsPlugin,
            state::StatePlugin,
            theme::ThemePlugin,
            audio::GameAudioPlugin,
            run::RunPlugin,
            world::WorldPlugin,
            player::PlayerPlugin,
            interaction::InteractionPlugin,
            ui::UiPlugin,
            capture::CapturePlugin,
        ))
        .add_plugins((music::MusicPlugin, cutscene::CutscenePlugin, hollow::HollowPlugin));
    }
}
